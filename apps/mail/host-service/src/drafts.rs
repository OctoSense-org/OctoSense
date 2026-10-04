//! Host-owned reply drafts and send attempts (ADR 0007).
//!
//! Agent methods can propose text and actions. Only Rust callers holding a
//! `Review` obtained by the trusted host review UI can consume authorization.
//! The shell MUST invalidate reviews on account changes and reject injected
//! input before calling `approve_and_send`. No JSON method mints authorization.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    OnceLock,
};

const BODY_MAX: usize = 8192;
const RECORD_MAX: usize = 512 * 1024;
const REVIEW_SECONDS: u64 = 600;
static SERIAL: Mutex<()> = Mutex::new(());
static GENERATION: AtomicU64 = AtomicU64::new(0);
type Changed = Arc<dyn Fn() + Send + Sync>;
fn changed_hook() -> &'static Mutex<Option<Changed>> {
    static HOOK: OnceLock<Mutex<Option<Changed>>> = OnceLock::new();
    HOOK.get_or_init(Default::default)
}
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}
/// Install a nonblocking UI signal callback. It must not reenter draft APIs.
pub fn on_change(callback: Option<Changed>) {
    *changed_hook().lock().unwrap_or_else(|e| e.into_inner()) = callback;
}
pub type ReviewRequested = Arc<dyn Fn(Review) -> Result<(), String> + Send + Sync>;
fn review_hook() -> &'static Mutex<Option<ReviewRequested>> {
    static HOOK: OnceLock<Mutex<Option<ReviewRequested>>> = OnceLock::new();
    HOOK.get_or_init(Default::default)
}
/// Route a foreground composer's review into the shell's trusted Mail region.
/// The callback may display/queue it; receiving it is not approval.
pub fn on_review_requested(callback: Option<ReviewRequested>) {
    *review_hook().lock().unwrap_or_else(|e| e.into_inner()) = callback;
}
pub type ClaimGuard = Arc<dyn Fn(&Path, &str, &str) -> Result<(), String> + Send + Sync>;
fn claim_guard() -> &'static Mutex<Option<ClaimGuard>> {
    static GUARD: OnceLock<Mutex<Option<ClaimGuard>>> = OnceLock::new();
    GUARD.get_or_init(Default::default)
}
/// Checked under the claim lock. This callback may read lifecycle state but
/// must not reenter draft APIs, invoke UI callbacks, or perform networking.
pub fn on_claim(callback: Option<ClaimGuard>) {
    *claim_guard().lock().unwrap_or_else(|e| e.into_inner()) = callback;
}
pub(crate) fn lock() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}
type Backend = (Arc<dyn Transport>, Arc<dyn Vault>);
fn backend() -> &'static Mutex<Option<Backend>> {
    static BACKEND: OnceLock<Mutex<Option<Backend>>> = OnceLock::new();
    BACKEND.get_or_init(Default::default)
}
pub(crate) fn set_backend(transport: Arc<dyn Transport>, vault: Arc<dyn Vault>) {
    *backend().lock().unwrap_or_else(|e| e.into_inner()) = Some((transport, vault));
}
fn configured() -> Result<Backend, String> {
    backend()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or("Mail service is not registered".into())
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn unique(kind: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{kind}-{}",
        &network::hash(&format!(
            "{}:{nanos}:{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))[..32]
    )
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 96 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
fn path(store: &Store, account: &str, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("Invalid draft identity".into());
    }
    let directory = store.dir.join(format!("drafts-{}", network::hash(account)));
    let file = directory.join(format!("{id}.json"));
    // The host root is supplied by Rust, never the app's writable workspace.
    // Refuse link substitution rather than following it outside that root.
    for dir in [
        store.dir.parent().ok_or("Invalid host root")?,
        store.dir.as_path(),
        directory.as_path(),
    ] {
        match std::fs::symlink_metadata(dir) {
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                return Err("Unsafe draft storage directory".into())
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
    }
    for target in [&file, &file.with_extension("tmp")] {
        match std::fs::symlink_metadata(target) {
            Ok(meta) => {
                if !meta.is_file() || meta.file_type().is_symlink() {
                    return Err("Unsafe draft storage file".into());
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if meta.nlink() != 1 {
                        return Err("Linked draft storage file is refused".into());
                    }
                }
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
    }
    Ok(file)
}
fn check_account(store: &Store, app: &str, account: &str) -> Result<Value, String> {
    if app != "os.mail" {
        return Err("Only Mail owns reply drafts".into());
    }
    store.granted(app, account)
}
fn save(store: &Store, draft: &Value) -> Result<(), String> {
    let path = path(store, text(draft, "account"), text(draft, "draft_id"))?;
    let bytes = serde_json::to_vec(draft).map_err(|e| e.to_string())?;
    if bytes.len() > RECORD_MAX {
        return Err("Draft history is full; keep this receipt and create a new reply".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
    }
    write_atomic(&path, &bytes)?;
    GENERATION.fetch_add(1, Ordering::Release);
    let signal = changed_hook()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if let Some(signal) = signal {
        signal();
    }
    Ok(())
}
fn active() -> &'static Mutex<BTreeSet<PathBuf>> {
    static ACTIVE: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();
    ACTIVE.get_or_init(Default::default)
}
fn load(store: &Store, app: &str, account: &str, id: &str) -> Result<Value, String> {
    check_account(store, app, account)?;
    let file = path(store, account, id)?;
    let bytes = std::fs::read(&file).map_err(|e| format!("Cannot read reply draft: {e}"))?;
    if bytes.len() > RECORD_MAX {
        return Err("Invalid draft record size".into());
    }
    let mut d: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid reply draft record")?;
    if d["schema"] != 1
        || d["publisher"] != app
        || d["account"] != account
        || d["draft_id"] != id
        || !d["attempts"].is_array()
        || !d["suggestions"].is_array()
    {
        return Err("Reply draft binding mismatch".into());
    }
    let rev = d["revision"]
        .as_u64()
        .filter(|r| *r > 0)
        .ok_or("Invalid draft revision")?;
    let states = [
        "draft",
        "awaiting_approval",
        "sending",
        "accepted",
        "failed_before_delivery",
        "outcome_unknown",
        "cancelled",
    ];
    if !states.contains(&text(&d, "status"))
        || !d["source_message"].is_object()
        || !d["email"].is_object()
        || [
            "from",
            "to",
            "subject",
            "body",
            "in_reply_to",
            "references",
            "chat_thread",
            "body_origin",
        ]
        .iter()
        .any(|k| !d[*k].is_string())
    {
        return Err("Invalid reply draft fields".into());
    }
    fields(text(&d, "to"), text(&d, "subject"), text(&d, "body"))?;
    let mut operations = BTreeSet::new();
    let attempts = d["attempts"].as_array().unwrap();
    if attempts.len() > 32
        || attempts.iter().any(|a| {
            !a.is_object()
                || !a["payload"].is_object()
                || !valid_id(text(a, "operation_id"))
                || !operations.insert(text(a, "operation_id"))
                || !states.contains(&text(a, "status"))
                || a["revision"].as_u64().is_none_or(|r| r == 0 || r > rev)
                || [
                    "from",
                    "to",
                    "subject",
                    "body",
                    "in_reply_to",
                    "references",
                    "message_id",
                ]
                .iter()
                .any(|k| !a["payload"][*k].is_string())
        })
    {
        return Err("Invalid reply attempt ledger".into());
    }
    if d["suggestions"].as_array().unwrap().len() > 8
        || d["suggestions"].as_array().unwrap().iter().any(|s| {
            !valid_id(text(s, "suggestion_id"))
                || !s["body"].is_string()
                || s["revision"].as_u64().is_none_or(|r| r == 0 || r > rev)
        })
    {
        return Err("Invalid reply suggestions".into());
    }
    // A process that no longer owns an in-flight operation cannot infer failure.
    if text(&d, "status") == "sending"
        && !active()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&file)
    {
        d["status"] = json!("outcome_unknown");
        if let Some(a) = d["attempts"].as_array_mut().and_then(|v| v.last_mut()) {
            a["status"] = json!("outcome_unknown");
            a["notice"] = json!(
                "The previous process stopped during submission. Delivery may have happened."
            );
        }
        save(store, &d)?;
    }
    Ok(d)
}
fn snapshot(d: &Value) -> Value {
    let mut v = d.clone();
    v.as_object_mut().unwrap().remove("schema");
    // Full snapshots are Rust-host-only; tools receive a bounded projection.
    v
}
fn revision(d: &Value, expected: u64) -> Result<(), String> {
    if d["revision"].as_u64() != Some(expected) {
        return Err("revision_conflict: the draft changed; keep unsaved text and reload".into());
    }
    Ok(())
}
fn fields(to: &str, subject: &str, body: &str) -> Result<(), String> {
    if to.is_empty()
        || to.len() > 254
        || !to.is_ascii()
        || to.split('@').count() != 2
        || to.starts_with('@')
        || to.ends_with('@')
        || to.chars().any(|c| c.is_whitespace() || c.is_control())
        || to.contains(['\r', '\n', '<', '>', ' ', ',', ';'])
    {
        return Err("Reply supports one To email address; no Cc/Bcc or recipient list".into());
    }
    if subject.len() > 512 || subject.contains(['\r', '\n', '\0']) {
        return Err("Subject must be at most 512 bytes without line breaks".into());
    }
    if body.len() > BODY_MAX || body.contains('\0') {
        return Err("Reply body must be at most 8192 UTF-8 bytes without NUL".into());
    }
    Ok(())
}
fn single_address(raw: &str) -> Result<String, String> {
    let parsed = mailparse::addrparse(raw).map_err(|_| "Cannot parse source reply recipient")?;
    if parsed.len() != 1 {
        return Err("The source requires multiple reply recipients, which are unsupported".into());
    }
    match &parsed[0] {
        mailparse::MailAddr::Single(address) => {
            fields(&address.addr, "", "")?;
            Ok(address.addr.clone())
        }
        _ => Err("Reply address groups are unsupported".into()),
    }
}
fn headers(raw: &str) -> Result<String, String> {
    if raw.len() > 4096 || raw.contains(['\r', '\n', '\0']) {
        return Err("Unsupported source reply headers".into());
    }
    Ok(raw.trim().to_string())
}
fn create(
    store: &Store,
    app: &str,
    account: &str,
    folder: &str,
    message: &str,
    body: &str,
    reply_key: &str,
) -> Result<Value, String> {
    let _guard = lock();
    let account_data = check_account(store, app, account)?;
    if folder.len() > 256 || message.len() > 128 {
        return Err("Invalid source identity".into());
    }
    if reply_key.len() > 64
        || !reply_key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err("reply_key must be at most 64 safe ASCII characters".into());
    }
    let mailbox = store.mailbox(account, folder);
    let source = mailbox["messages"]
        .as_array()
        .and_then(|rows| rows.iter().find(|m| text(m, "id") == message))
        .ok_or("There is no such source message")?;
    let generation = mailbox["state"]["uidvalidity"].clone();
    let mut identity = json!([account, folder, message, generation, source["message_id"]]);
    if !reply_key.is_empty() {
        identity.as_array_mut().unwrap().push(json!(reply_key));
    }
    let id = format!("draft-{}", &network::hash(&identity.to_string())[..32]);
    if path(store, account, &id)?
        .try_exists()
        .map_err(|e| e.to_string())?
    {
        return load(store, app, account, &id).map(|d| snapshot(&d));
    }
    let recipient = single_address(if text(source, "reply_to").trim().is_empty() {
        text(source, "address")
    } else {
        text(source, "reply_to")
    })?;
    let old_subject = text(source, "subject");
    let subject = if old_subject.to_ascii_lowercase().starts_with("re:") {
        old_subject.to_string()
    } else {
        format!("Re: {old_subject}")
    };
    fields(&recipient, &subject, body)?;
    let in_reply_to = headers(text(source, "message_id"))?;
    let old_refs = headers(text(source, "references"))?;
    let references = if in_reply_to.is_empty() {
        old_refs
    } else {
        format!("{old_refs} {in_reply_to}").trim().to_string()
    };
    let d = json!({"schema":1,"publisher":app,"account":account,"from":account_data["address"],"draft_id":id,"revision":1,
        "chat_thread":format!("reply-{}", &network::hash(&id)[..24]),
        "source_message":{"folder":folder,"message":message,"generation":generation,"internet_message_id":in_reply_to},
        "email":{"sender":clip(text(source,"sender"),160),"address":clip(text(source,"address"),254),"subject":clip(old_subject,512),"body":clip(text(source,"body"),4000)},
        "to":recipient,"subject":subject,"body":body,"body_origin":"model","in_reply_to":in_reply_to,"references":references,
        "status":"draft","suggestions":[],"attempts":[]});
    save(store, &d)?;
    Ok(snapshot(&d))
}

/// Read the full bound draft, including email context and durable attempt receipts.
pub fn read(host_dir: &Path, app: &str, account: &str, draft_id: &str) -> Result<Value, String> {
    let (_, vault) = configured()?;
    let _guard = lock();
    load(&Store::at(host_dir, vault), app, account, draft_id).map(|d| snapshot(&d))
}
/// Trusted editor update. This method is not exposed to scripts or agents.
/// Only `to`, `subject` and `body` may appear in `changes`.
pub fn update(
    host_dir: &Path,
    app: &str,
    account: &str,
    draft_id: &str,
    expected_revision: u64,
    changes: &Value,
) -> Result<Value, String> {
    let (_, vault) = configured()?;
    update_in(
        &Store::at(host_dir, vault),
        app,
        account,
        draft_id,
        expected_revision,
        changes,
        None,
    )
}
#[allow(clippy::too_many_arguments)]
fn update_in(
    store: &Store,
    app: &str,
    account: &str,
    id: &str,
    expected: u64,
    changes: &Value,
    suggestion: Option<&str>,
) -> Result<Value, String> {
    let _guard = lock();
    let mut d = load(store, app, account, id)?;
    revision(&d, expected)?;
    if d["attempts"].as_array().unwrap().iter().any(|a| {
        matches!(
            text(a, "status"),
            "sending" | "accepted" | "outcome_unknown" | "failed_before_delivery"
        )
    }) {
        return Err("This draft has a submission attempt; its payload is frozen".into());
    }
    let object = changes
        .as_object()
        .ok_or("Draft changes must be an object")?;
    if object.is_empty()
        || object
            .keys()
            .any(|k| !matches!(k.as_str(), "to" | "subject" | "body"))
    {
        return Err("Only recipient, subject and body can be edited".into());
    }
    if let Some(sid) = suggestion {
        let s = d["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["suggestion_id"] == sid)
            .ok_or("No such suggestion")?;
        if s["revision"].as_u64() != Some(expected) || s["body"] != changes["body"] {
            return Err("Suggestion is stale".into());
        }
    }
    for (k, v) in object {
        if !v.is_string() {
            return Err("Draft fields must be text".into());
        }
        d[k] = v.clone();
    }
    fields(text(&d, "to"), text(&d, "subject"), text(&d, "body"))?;
    d["revision"] = json!(expected.checked_add(1).ok_or("Revision limit reached")?);
    d["body_origin"] = json!(if suggestion.is_some() {
        "model_accepted"
    } else if object.contains_key("body") {
        "user"
    } else {
        text(&d, "body_origin")
    });
    d["status"] = json!("draft");
    for a in d["attempts"].as_array_mut().unwrap() {
        if a["status"] == "awaiting_approval" {
            a["status"] = json!("cancelled");
        }
    }
    d["suggestions"] = json!([]);
    save(store, &d)?;
    Ok(snapshot(&d))
}
/// Accept a model suggestion only if the person is still viewing its base revision.
pub fn accept_suggestion(
    host_dir: &Path,
    app: &str,
    account: &str,
    draft_id: &str,
    suggestion_id: &str,
    expected_revision: u64,
) -> Result<Value, String> {
    let (_, vault) = configured()?;
    let store = Store::at(host_dir, vault);
    let body = {
        let _guard = lock();
        let d = load(&store, app, account, draft_id)?;
        d["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["suggestion_id"] == suggestion_id)
            .ok_or("No such suggestion")?["body"]
            .clone()
    };
    update_in(
        &store,
        app,
        account,
        draft_id,
        expected_revision,
        &json!({"body":body}),
        Some(suggestion_id),
    )
}
fn suggest(
    store: &Store,
    app: &str,
    account: &str,
    id: &str,
    expected: u64,
    body: &str,
) -> Result<Value, String> {
    let _guard = lock();
    let mut d = load(store, app, account, id)?;
    revision(&d, expected)?;
    fields(text(&d, "to"), text(&d, "subject"), body)?;
    if !matches!(text(&d, "status"), "draft" | "awaiting_approval") {
        return Err("This reply is no longer editable".into());
    }
    let suggestions = d["suggestions"].as_array_mut().unwrap();
    if suggestions.len() >= 8 {
        suggestions.remove(0);
    }
    suggestions.push(json!({"suggestion_id":unique("suggestion"),"revision":expected,"body":body,"origin":"model"}));
    save(store, &d)?;
    Ok(snapshot(&d))
}
fn propose(
    store: &Store,
    app: &str,
    account: &str,
    id: &str,
    expected: u64,
) -> Result<Value, String> {
    let _guard = lock();
    let mut d = load(store, app, account, id)?;
    revision(&d, expected)?;
    if let Some(a) = d["attempts"].as_array().unwrap().last() {
        if a["revision"].as_u64() == Some(expected) && a["status"] != "cancelled" {
            return Ok(snapshot(&d));
        }
    }
    add_attempt(&mut d, None)?;
    save(store, &d)?;
    Ok(snapshot(&d))
}
fn add_attempt(d: &mut Value, prior: Option<&str>) -> Result<(), String> {
    if d["attempts"].as_array().unwrap().len() >= 32 {
        return Err("Reply attempt limit reached".into());
    }
    fields(text(d, "to"), text(d, "subject"), text(d, "body"))?;
    let operation = unique("send");
    let domain = text(d, "from")
        .split('@')
        .nth(1)
        .unwrap_or("octosense.local");
    let payload = json!({"from":d["from"],"to":d["to"],"subject":d["subject"],"body":d["body"],"in_reply_to":d["in_reply_to"],"references":d["references"],"message_id":format!("<{operation}@{domain}>")});
    let attempt = json!({"operation_id":operation,"revision":d["revision"],"status":"awaiting_approval","payload":payload,"prior_attempt":prior});
    d["attempts"].as_array_mut().unwrap().push(attempt);
    d["status"] = json!("awaiting_approval");
    Ok(())
}

/// Opaque, process-local, single-use review capability. It is never serialized
/// into card data. Its snapshot is exactly what the trusted UI must display.
pub struct Review {
    token: String,
    snapshot: Value,
}
impl Review {
    pub fn snapshot(&self) -> &Value {
        &self.snapshot
    }
}
struct ReviewState {
    host_dir: PathBuf,
    app: String,
    account: String,
    draft: String,
    operation: String,
    revision: u64,
    expires: u64,
    snapshot: Value,
}
fn reviews() -> &'static Mutex<BTreeMap<String, ReviewState>> {
    static REVIEWS: OnceLock<Mutex<BTreeMap<String, ReviewState>>> = OnceLock::new();
    REVIEWS.get_or_init(Default::default)
}
fn review_from(host_dir: &Path, app: &str, account: &str, d: &Value) -> Result<Review, String> {
    let a = d["attempts"]
        .as_array()
        .and_then(|a| a.last())
        .ok_or("No send proposal")?;
    if a["status"] != "awaiting_approval" {
        return Err("This attempt is not awaiting approval; inspect its receipt".into());
    }
    let prior_status = d["attempts"].as_array().unwrap().iter()
        .find(|prior| !a["prior_attempt"].is_null() && prior["operation_id"] == a["prior_attempt"])
        .map(|prior| prior["status"].clone()).unwrap_or(Value::Null);
    let snapshot = json!({"publisher":app,"account":account,"draft_id":d["draft_id"],"revision":d["revision"],"source_message":d["source_message"],"operation_id":a["operation_id"],"payload":a["payload"],"prior_attempt":a["prior_attempt"],"prior_status":prior_status});
    let token = unique("review");
    let mut all = reviews().lock().unwrap_or_else(|e| e.into_inner());
    all.retain(|_, r| r.expires > now());
    if all.len() >= 128 {
        return Err("Too many open reviews".into());
    }
    all.insert(
        token.clone(),
        ReviewState {
            host_dir: host_dir.to_path_buf(),
            app: app.into(),
            account: account.into(),
            draft: text(d, "draft_id").into(),
            operation: text(a, "operation_id").into(),
            revision: d["revision"].as_u64().ok_or("Invalid revision")?,
            expires: now() + REVIEW_SECONDS,
            snapshot: snapshot.clone(),
        },
    );
    Ok(Review { token, snapshot })
}
pub fn prepare_review(
    host_dir: &Path,
    app: &str,
    account: &str,
    draft_id: &str,
    expected_revision: u64,
) -> Result<Review, String> {
    let (_, vault) = configured()?;
    let store = Store::at(host_dir, vault);
    propose(&store, app, account, draft_id, expected_revision)?;
    let _guard = lock();
    let d = load(&store, app, account, draft_id)?;
    revision(&d, expected_revision)?;
    review_from(host_dir, app, account, &d)
}
/// Explicit user Retry only. A model repeating propose_send cannot reach this.
pub fn retry_review(
    host_dir: &Path,
    app: &str,
    account: &str,
    draft_id: &str,
    prior_attempt: &str,
    acknowledge_duplicate: bool,
) -> Result<Review, String> {
    let (_, vault) = configured()?;
    let store = Store::at(host_dir, vault);
    retry_in(
        &store,
        app,
        account,
        draft_id,
        prior_attempt,
        acknowledge_duplicate,
    )
}
fn retry_in(
    store: &Store,
    app: &str,
    account: &str,
    draft_id: &str,
    prior_attempt: &str,
    acknowledge_duplicate: bool,
) -> Result<Review, String> {
    let _guard = lock();
    let mut d = load(store, app, account, draft_id)?;
    let a = d["attempts"]
        .as_array()
        .and_then(|a| a.last())
        .ok_or("No previous attempt")?;
    if a["operation_id"] != prior_attempt
        || !matches!(
            text(a, "status"),
            "failed_before_delivery" | "outcome_unknown"
        )
    {
        return Err("Only the last failed or uncertain attempt can be retried".into());
    }
    if a["status"] == "outcome_unknown" && !acknowledge_duplicate {
        return Err("Acknowledge the possible duplicate before retrying uncertain delivery".into());
    }
    add_attempt(&mut d, Some(prior_attempt))?;
    save(store, &d)?;
    review_from(
        store.dir.parent().ok_or("Invalid host root")?,
        app,
        account,
        &d,
    )
}
/// Invalidate all capabilities for an account when the shell changes selection.
pub fn invalidate_reviews(host_dir: &Path, account: &str) {
    let _guard = lock();
    invalidate_locked(host_dir, Some(account));
}
/// Rebinding Mail to a newly signed-in account invalidates every old review.
pub fn invalidate_all_reviews(host_dir: &Path) {
    let _guard = lock();
    invalidate_locked(host_dir, None);
}
pub(crate) fn invalidate_locked(host_dir: &Path, account: Option<&str>) {
    reviews()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|_, r| r.host_dir != host_dir || account.is_some_and(|a| r.account != a));
}
pub fn cancel_review(review: Review) -> Result<(), String> {
    let (_, vault) = configured()?;
    cancel_in(review, vault)
}
/// Retire one view's capability without cancelling the shared attempt another
/// view may currently be reviewing. Explicit Cancel uses `cancel_review`.
pub fn revoke_review(review: Review) {
    let _guard = lock();
    reviews()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&review.token);
}
fn cancel_in(review: Review, vault: Arc<dyn Vault>) -> Result<(), String> {
    let _guard = lock();
    let Some(r) = reviews()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&review.token)
    else {
        return Ok(());
    };
    let store = Store::at(&r.host_dir, vault);
    let mut d = load(&store, &r.app, &r.account, &r.draft)?;
    if let Some(a) = d["attempts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|a| a["operation_id"] == r.operation)
    {
        if a["status"] == "awaiting_approval" {
            a["status"] = json!("cancelled");
            d["status"] = json!("draft");
            save(&store, &d)?;
        }
    }
    Ok(())
}
/// Transport outcome classification. Unknown is deliberately the default for
/// third-party/test transports returning an ordinary error.
#[derive(Debug, Clone)]
pub enum SendFailure {
    BeforeDelivery(String),
    Unknown(String),
}
/// Blocking; invoke off the UI thread, ONLY after a trusted real activation of
/// the host review control. Neither developer mode nor script callers use it.
pub fn approve_and_send(review: Review) -> Result<Value, String> {
    let (transport, vault) = configured()?;
    execute(review, transport.as_ref(), vault)
}
fn execute(
    review: Review,
    transport: &dyn Transport,
    vault: Arc<dyn Vault>,
) -> Result<Value, String> {
    let (r, store, account, payload, file) = {
        let _guard = lock();
        let r = reviews()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&review.token)
            .ok_or("Review was cancelled, expired or already consumed")?;
        if r.expires <= now() || r.snapshot != review.snapshot {
            return Err("Review expired or snapshot changed".into());
        }
        let store = Store::at(&r.host_dir, vault);
        let guard = claim_guard()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(guard) = guard {
            guard(&r.host_dir, &r.app, &r.account)?;
        }
        let mut d = load(&store, &r.app, &r.account, &r.draft)?;
        revision(&d, r.revision)?;
        let a = d["attempts"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|a| a["operation_id"] == r.operation)
            .ok_or("Unknown send operation")?;
        if a["status"] != "awaiting_approval" {
            return Ok(snapshot(&d));
        }
        if a["payload"] != r.snapshot["payload"] {
            return Err("Outbound snapshot changed".into());
        }
        let account = store.account_for(&r.app, &r.account)?;
        if account["address"] != a["payload"]["from"] {
            return Err("Sending account changed; review again".into());
        }
        let payload = a["payload"].clone();
        a["status"] = json!("sending");
        d["status"] = json!("sending");
        save(&store, &d)?; // Failure here never enters the transport.
        let file = path(&store, &r.account, &r.draft)?;
        active()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(file.clone());
        (r, store, account, payload, file)
    };
    // No draft/account lock is held across networking; sign-out remains usable.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        transport.send_checked(&account, &payload)
    }))
    .unwrap_or_else(|_| {
        Err(SendFailure::Unknown(
            "Transport stopped without a result".into(),
        ))
    });
    let _guard = lock();
    // Read before releasing ACTIVE so this live result is not called a crash.
    let loaded = load(&store, &r.app, &r.account, &r.draft);
    active()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&file);
    let mut d=loaded.map_err(|_|"Submission finished after account removal; no deleted draft was recreated. Check provider Sent before any retry.".to_string())?;
    let a = d["attempts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|a| a["operation_id"] == r.operation)
        .ok_or("Missing submission receipt")?;
    let status = match outcome {
        Ok(receipt) if receipt["accepted"] == true => {
            a["receipt"] = receipt;
            "accepted"
        }
        Ok(_) => {
            a["notice"] = json!("Transport returned no authoritative acceptance");
            "outcome_unknown"
        }
        Err(SendFailure::BeforeDelivery(_)) => {
            a["notice"]=json!("Submission failed before message delivery. Explicit Retry requires a new approval.");
            "failed_before_delivery"
        }
        Err(SendFailure::Unknown(_)) => {
            a["notice"] =
                json!("Delivery may have happened. Check Sent; do not automatically resend.");
            "outcome_unknown"
        }
    };
    a["status"] = json!(status);
    d["status"] = json!(status);
    save(&store, &d)?;
    if status == "accepted" {
        contacts::record_sent(&store.dir, &r.account, text(&payload, "to"));
    }
    Ok(snapshot(&d))
}

/// Projection under the kernel's 4 KiB result ceiling. Full editor/review data
/// remains available to the Rust host; long model-visible text is explicit.
fn for_agent(d: &Value) -> Value {
    let body = text(d, "body");
    let subject = text(d, "subject");
    let mut v = json!({"draft_id":d["draft_id"],"revision":d["revision"],"account":d["account"],
        "source_message":{"folder":d["source_message"]["folder"],"message":d["source_message"]["message"],"generation":d["source_message"]["generation"]},
        "chat_thread":d["chat_thread"],"to":d["to"],"subject":clip(subject,512),"subject_truncated":subject.len()>clip(subject,512).len(),
        "body":clip(body,1800),"body_truncated":body.len()>clip(body,1800).len(),"body_origin":d["body_origin"],"status":d["status"]});
    if let Some(a) = d["attempts"].as_array().and_then(|a| a.last()) {
        v["attempt"] = json!({"operation_id":a["operation_id"],"status":a["status"],"message_id":a["payload"]["message_id"]});
    }
    if let Some(s) = d["suggestions"].as_array().and_then(|a| a.last()) {
        v["suggestion"] = json!({"suggestion_id":s["suggestion_id"],"revision":s["revision"],"pending_user_acceptance":true});
    }
    v
}
pub(crate) fn agent_call(
    store: &Store,
    app: &str,
    method: &str,
    args: &Value,
) -> Result<Value, String> {
    let allowed: &[&str] = match method {
        "propose_reply" => &["account", "folder", "message", "body", "reply_key"],
        "draft" => &["account", "draft_id"],
        "suggest_reply" => &["account", "draft_id", "expected_revision", "body"],
        "propose_send" => &["account", "draft_id", "expected_revision"],
        _ => return Err("Unknown reply tool".into()),
    };
    if args
        .as_object()
        .is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err("Unsupported reply arguments; attachments, Cc/Bcc and forwarded content are not supported".into());
    }
    let account = text(args, "account");
    let id = text(args, "draft_id");
    let expected = || {
        args["expected_revision"]
            .as_u64()
            .ok_or_else(|| "expected_revision must be an integer".to_string())
    };
    let d = match method {
        "propose_reply" => create(
            store,
            app,
            account,
            if text(args, "folder").is_empty() {
                INBOX
            } else {
                text(args, "folder")
            },
            text(args, "message"),
            args["body"].as_str().ok_or("body must be text")?,
            match args.get("reply_key") {
                Some(key) => key
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("reply_key must be nonempty text when supplied")?,
                None => "",
            },
        )?,
        "draft" => {
            let _guard = lock();
            load(store, app, account, id)?
        }
        "suggest_reply" => suggest(
            store,
            app,
            account,
            id,
            expected()?,
            args["body"].as_str().ok_or("body must be text")?,
        )?,
        "propose_send" => propose(store, app, account, id, expected()?)?,
        _ => return Err("Unknown reply tool".into()),
    };
    let v = for_agent(&d);
    if serde_json::to_vec(&v).map_err(|e| e.to_string())?.len() > 3800 {
        return Err("Reply metadata exceeds the model response budget".into());
    }
    Ok(v)
}
/// Foreground script composer: stage data and request host review, never approve.
pub(crate) fn review_composer(store: &Store, app: &str, args: &Value) -> Result<Value, String> {
    let hook = review_hook()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or("This host has no Mail review surface")?;
    let d = compose_draft(store, app, args)?;
    let account = text(&d, "account");
    let id = text(&d, "draft_id");
    let host_dir = store.dir.parent().ok_or("Invalid host root")?;
    propose(
        store,
        app,
        account,
        id,
        d["revision"].as_u64().ok_or("Invalid revision")?,
    )?;
    let review = {
        let _guard = lock();
        let current = load(store, app, account, id)?;
        revision(&current, d["revision"].as_u64().ok_or("Invalid revision")?)?;
        review_from(host_dir, app, account, &current)?
    };
    // A callback that rejects opening must not leave a usable capability, even
    // if it retained the value before returning an error.
    let cleanup = Review {
        token: review.token.clone(),
        snapshot: review.snapshot.clone(),
    };
    if let Err(error) = hook(review) {
        cancel_in(cleanup, store.vault.clone())?;
        return Err(error);
    }
    Ok(
        json!({"review_required":true,"compose_id":d["compose_id"],"draft_id":id,"revision":d["revision"]}),
    )
}
fn compose_draft(store: &Store, app: &str, args: &Value) -> Result<Value, String> {
    let allowed = [
        "account",
        "to",
        "subject",
        "body",
        "compose_id",
        "expected_revision",
        "folder",
        "message",
    ];
    if args
        .as_object()
        .is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err("Unsupported composer fields: one To address and plain text only".into());
    }
    let _guard = lock();
    let account = text(args, "account");
    let granted = check_account(store, app, account)?;
    let to = args["to"].as_str().ok_or("to must be text")?;
    let subject = args["subject"].as_str().ok_or("subject must be text")?;
    let body = args["body"].as_str().ok_or("body must be text")?;
    fields(to, subject, body)?;
    let supplied = text(args, "compose_id");
    let compose_id = if supplied.is_empty() {
        unique("compose")
    } else {
        if !valid_id(supplied) || !supplied.starts_with("compose-") {
            return Err("Invalid composer identity".into());
        }
        supplied.to_string()
    };
    let id = format!(
        "draft-{}",
        &network::hash(&format!("{account}:{compose_id}"))[..32]
    );
    let mut d = if supplied.is_empty() {
        let message = text(args, "message");
        let folder = if text(args, "folder").is_empty() {
            INBOX
        } else {
            text(args, "folder")
        };
        if folder.len() > 256 || message.len() > 128 {
            return Err("Invalid reply source".into());
        }
        let (source_message, email, in_reply_to, references) = if message.is_empty() {
            (
                json!({"folder":null,"message":null,"generation":null}),
                json!({}),
                String::new(),
                String::new(),
            )
        } else {
            let mailbox = store.mailbox(account, folder);
            let source = mailbox["messages"]
                .as_array()
                .and_then(|a| a.iter().find(|m| text(m, "id") == message))
                .ok_or("There is no such reply source")?;
            // Validate unsupported Reply-To even if the composer supplied an
            // edited To field; silently reducing a group is never acceptable.
            single_address(if text(source, "reply_to").trim().is_empty() {
                text(source, "address")
            } else {
                text(source, "reply_to")
            })?;
            let original = headers(text(source, "message_id"))?;
            let refs = format!("{} {}", headers(text(source, "references"))?, original)
                .trim()
                .to_string();
            (
                json!({"folder":folder,"message":message,"generation":mailbox["state"]["uidvalidity"],"internet_message_id":original}),
                json!({"sender":clip(text(source,"sender"),160),"address":clip(text(source,"address"),254),"subject":clip(text(source,"subject"),512),"body":clip(text(source,"body"),4000)}),
                original,
                refs,
            )
        };
        json!({"schema":1,"publisher":app,"account":account,"from":granted["address"],"draft_id":id,"compose_id":compose_id,"revision":1,
            "chat_thread":format!("reply-{}",&network::hash(&id)[..24]),"source_message":source_message,"email":email,
            "to":to,"subject":subject,"body":body,"body_origin":"app","in_reply_to":in_reply_to,"references":references,"status":"draft","suggestions":[],"attempts":[]})
    } else {
        let mut d = load(store, app, account, &id)?;
        if d["compose_id"] != compose_id {
            return Err("Composer binding mismatch".into());
        }
        let expected = args["expected_revision"]
            .as_u64()
            .ok_or("expected_revision is required for an existing composer")?;
        revision(&d, expected)?;
        let message = text(args, "message");
        let folder = if text(args, "folder").is_empty() {
            INBOX
        } else {
            text(args, "folder")
        };
        if text(&d["source_message"], "message") != message
            || (!message.is_empty() && text(&d["source_message"], "folder") != folder)
        {
            return Err("An existing composer cannot change its source message".into());
        }
        if d["attempts"].as_array().unwrap().iter().any(|a| {
            matches!(
                text(a, "status"),
                "sending" | "accepted" | "failed_before_delivery" | "outcome_unknown"
            )
        }) {
            return Err("This message has a submission attempt; inspect its receipt instead of composing again".into());
        }
        if d["to"] != to || d["subject"] != subject || d["body"] != body {
            d["revision"] = json!(expected.checked_add(1).ok_or("Revision limit reached")?);
            for a in d["attempts"].as_array_mut().unwrap() {
                if a["status"] == "awaiting_approval" {
                    a["status"] = json!("cancelled");
                }
            }
            d["status"] = json!("draft");
            d["suggestions"] = json!([]);
        }
        d
    };
    d["to"] = json!(to);
    d["subject"] = json!(subject);
    d["body"] = json!(body);
    d["body_origin"] = json!("app");
    save(store, &d)?;
    Ok(snapshot(&d))
}

/// Only the service calls this, after checking the authenticated tool account.
pub(crate) fn publication_binding(
    store: &Store,
    app: &str,
    account: &str,
    id: &str,
) -> Result<Value, String> {
    let _guard = lock();
    let d = load(store, app, account, id)?;
    Ok(
        json!({"publisher":app,"account":account,"source_message":d["source_message"],"draft_id":d["draft_id"],"draft_revision":d["revision"],"chat_thread":d["chat_thread"]}),
    )
}
/// Account data deletion, called under the draft lock by remove_account.
pub(crate) fn forget(store: &Store, account: &str) {
    let _ = std::fs::remove_dir_all(store.dir.join(format!("drafts-{}", network::hash(account))));
    reviews()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|_, r| r.host_dir.join("mail") != store.dir || r.account != account);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct TestVault;
    impl Vault for TestVault {
        fn put(&self, _: &vault::Place, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn get(&self, _: &vault::Place, _: &str) -> Result<String, String> {
            Ok("test-only-password".into())
        }
        fn remove(&self, _: &vault::Place, _: &str) {}
    }
    struct Fake {
        calls: AtomicUsize,
        sent: Mutex<Vec<Value>>,
        failure: Mutex<Option<SendFailure>>,
    }
    impl Default for Fake {
        fn default() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                sent: Mutex::new(Vec::new()),
                failure: Mutex::new(None),
            }
        }
    }
    impl Transport for Fake {
        fn test(&self, _: &Value) -> Result<(), String> {
            Ok(())
        }
        fn folders(&self, _: &Value) -> Result<Vec<Value>, String> {
            Ok(vec![])
        }
        fn fetch(&self, _: &Value, _: &str, _: &Value) -> Result<Value, String> {
            Ok(json!({}))
        }
        fn mark_seen(&self, _: &Value, _: &str, _: &Value) -> Result<(), String> {
            Ok(())
        }
        fn send(&self, _: &Value, draft: &Value) -> Result<Value, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.sent.lock().unwrap().push(draft.clone());
            Ok(json!({"accepted":true,"message_id":draft["message_id"]}))
        }
        fn send_checked(&self, account: &Value, draft: &Value) -> Result<Value, SendFailure> {
            if let Some(f) = self.failure.lock().unwrap().take() {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.sent.lock().unwrap().push(draft.clone());
                Err(f)
            } else {
                self.send(account, draft).map_err(SendFailure::Unknown)
            }
        }
    }
    struct Fixture {
        root: PathBuf,
        store: Store,
        transport: Fake,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(unique("mail-drafts-test"));
            let store = Store::at(&root, Arc::new(TestVault));
            store
                .save_accounts(&[
                    json!({"id":"one","address":"me@example.com","apps":["os.mail"]}),
                    json!({"id":"two","address":"other@example.com","apps":["os.mail"]}),
                ])
                .unwrap();
            store.save_mailbox("one",INBOX,&json!({"state":{"uidvalidity":44},"messages":[{"id":"message-one","message_id":"<original@example.com>","reply_to":"Reply desk <reply@example.com>","address":"sender@example.com","subject":"Appointment","body":"Original email","references":"<earlier@example.com>"}]})).unwrap();
            Self {
                root,
                store,
                transport: Fake::default(),
            }
        }
        fn create(&self) -> Value {
            create(
                &self.store,
                "os.mail",
                "one",
                INBOX,
                "message-one",
                "Thanks, I can attend.",
                "",
            )
            .unwrap()
        }
        fn read(&self, id: &str) -> Value {
            let _guard = lock();
            snapshot(&load(&self.store, "os.mail", "one", id).unwrap())
        }
        fn review(&self, id: &str, rev: u64) -> Review {
            propose(&self.store, "os.mail", "one", id, rev).unwrap();
            let _guard = lock();
            let d = load(&self.store, "os.mail", "one", id).unwrap();
            review_from(&self.root, "os.mail", "one", &d).unwrap()
        }
        fn send(&self, r: Review) -> Result<Value, String> {
            execute(r, &self.transport, Arc::new(TestVault))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            invalidate_reviews(&self.root, "one");
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn reply_uses_cached_headers_and_deduplicates_creation_without_overwriting_text() {
        let f = Fixture::new();
        let d = f.create();
        assert_eq!(d["to"], "reply@example.com");
        assert_eq!(d["subject"], "Re: Appointment");
        assert_eq!(d["in_reply_to"], "<original@example.com>");
        assert_eq!(
            d["references"],
            "<earlier@example.com> <original@example.com>"
        );
        assert_eq!(d["source_message"]["generation"], 44);
        let same = create(
            &f.store,
            "os.mail",
            "one",
            INBOX,
            "message-one",
            "Model retry must not overwrite",
            "",
        )
        .unwrap();
        assert_eq!(same, d);
        assert_eq!(f.read(text(&d, "draft_id")), d, "durable roundtrip");
    }
    #[test]
    fn cas_edits_invalidate_approval_and_stale_model_suggestions() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        let suggestion = suggest(&f.store, "os.mail", "one", id, 1, "Agent alternative").unwrap();
        let sid = text(&suggestion["suggestions"][0], "suggestion_id");
        let review = f.review(id, 1);
        let edited = update_in(
            &f.store,
            "os.mail",
            "one",
            id,
            1,
            &json!({"body":"My own edit"}),
            None,
        )
        .unwrap();
        assert_eq!(edited["revision"], 2);
        assert_eq!(edited["body_origin"], "user");
        assert!(update_in(
            &f.store,
            "os.mail",
            "one",
            id,
            1,
            &json!({"body":"Agent alternative"}),
            Some(sid)
        )
        .unwrap_err()
        .contains("revision_conflict"));
        assert!(f.send(review).unwrap_err().contains("revision_conflict"));
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn accepting_suggestion_is_separate_from_proposal_and_preserves_ai_origin() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        let s = suggest(&f.store, "os.mail", "one", id, 1, "Suggested text").unwrap();
        assert_eq!(s["body"], d["body"]);
        assert_eq!(s["revision"], 1);
        let accepted = update_in(
            &f.store,
            "os.mail",
            "one",
            id,
            1,
            &json!({"body":"Suggested text"}),
            Some(text(&s["suggestions"][0], "suggestion_id")),
        )
        .unwrap();
        assert_eq!(accepted["revision"], 2);
        assert_eq!(accepted["body_origin"], "model_accepted");
        assert_eq!(accepted["status"], "draft");
    }
    #[test]
    fn handles_cannot_cross_account_app_or_path_and_extra_transport_fields_are_rejected() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        let _guard = lock();
        assert!(load(&f.store, "os.mail", "two", id).is_err());
        assert!(load(&f.store, "os.other", "one", id).is_err());
        assert!(load(&f.store, "os.mail", "one", "../accounts").is_err());
        drop(_guard);
        assert!(agent_call(
            &f.store,
            "os.mail",
            "propose_reply",
            &json!({"account":"one","message":"message-one","body":"x","cc":"third@example.com"})
        )
        .is_err());
        assert!(update_in(
            &f.store,
            "os.mail",
            "one",
            id,
            1,
            &json!({"to":"a@example.com,b@example.com"}),
            None
        )
        .is_err());
    }
    #[test]
    fn immutable_attempt_has_one_send_and_survives_repeated_proposals() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        let first = f.review(id, 1);
        let duplicate = f.review(id, 1);
        assert_eq!(first.snapshot(), duplicate.snapshot());
        let sent = f.send(first).unwrap();
        assert_eq!(sent["status"], "accepted");
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 1);
        assert_eq!(f.send(duplicate).unwrap()["status"], "accepted");
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 1);
        let again = propose(&f.store, "os.mail", "one", id, 1).unwrap();
        assert_eq!(again["attempts"].as_array().unwrap().len(), 1);
        let payload = &f.transport.sent.lock().unwrap()[0];
        assert_eq!(
            payload["message_id"],
            sent["attempts"][0]["payload"]["message_id"]
        );
        assert_eq!(payload["in_reply_to"], "<original@example.com>");
    }
    #[test]
    fn revoked_account_and_cancelled_or_expired_capabilities_never_send() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        let cancelled = f.review(id, 1);
        invalidate_reviews(&f.root, "one");
        assert!(f.send(cancelled).is_err());
        let expired = f.review(id, 1);
        reviews()
            .lock()
            .unwrap()
            .get_mut(&expired.token)
            .unwrap()
            .expires = 0;
        assert!(f.send(expired).unwrap_err().contains("expired"));
        let removed = f.review(id, 1);
        {
            let _guard = lock();
            f.store.save_accounts(&[]).unwrap();
        }
        assert!(f.send(removed).is_err());
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn unknown_error_and_restart_are_not_automatic_retries() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        *f.transport.failure.lock().unwrap() =
            Some(SendFailure::Unknown("private transport detail".into()));
        let result = f.send(f.review(id, 1)).unwrap();
        assert_eq!(result["status"], "outcome_unknown");
        assert!(!result.to_string().contains("private transport detail"));
        let proposed = propose(&f.store, "os.mail", "one", id, 1).unwrap();
        assert_eq!(proposed["status"], "outcome_unknown");
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 1);
        {
            let _guard = lock();
            let mut d = load(&f.store, "os.mail", "one", id).unwrap();
            d["status"] = json!("sending");
            d["attempts"][0]["status"] = json!("sending");
            save(&f.store, &d).unwrap();
        }
        let recovered = f.read(id);
        assert_eq!(recovered["status"], "outcome_unknown");
        assert_eq!(
            recovered["attempts"][0]["payload"]["message_id"],
            result["attempts"][0]["payload"]["message_id"]
        );
    }
    #[test]
    fn known_failure_can_get_new_linked_attempt_but_normal_propose_cannot_retry() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        *f.transport.failure.lock().unwrap() =
            Some(SendFailure::BeforeDelivery("connection refused".into()));
        let failed = f.send(f.review(id, 1)).unwrap();
        assert_eq!(failed["status"], "failed_before_delivery");
        let repeated = propose(&f.store, "os.mail", "one", id, 1).unwrap();
        assert_eq!(repeated["attempts"].as_array().unwrap().len(), 1);
        let retry = {
            let _guard = lock();
            let mut d = load(&f.store, "os.mail", "one", id).unwrap();
            add_attempt(&mut d, Some(text(&failed["attempts"][0], "operation_id"))).unwrap();
            save(&f.store, &d).unwrap();
            review_from(&f.root, "os.mail", "one", &d).unwrap()
        };
        assert_ne!(
            retry.snapshot()["payload"]["message_id"],
            failed["attempts"][0]["payload"]["message_id"]
        );
        assert_eq!(
            retry.snapshot()["prior_attempt"],
            failed["attempts"][0]["operation_id"]
        );
        assert_eq!(f.send(retry).unwrap()["status"], "accepted");
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn failure_to_persist_claim_prevents_transport() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        let r = f.review(id, 1);
        let file = path(&f.store, "one", id).unwrap();
        std::fs::create_dir(file.with_extension("tmp")).unwrap();
        assert!(f.send(r).is_err());
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_draft_files_are_refused_without_mutating_target() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        let file = path(&f.store, "one", id).unwrap();
        let target = f.root.join("unrelated");
        std::fs::write(&target, "unchanged").unwrap();
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&target, &file).unwrap();
        {
            let _guard = lock();
            assert!(load(&f.store, "os.mail", "one", id)
                .unwrap_err()
                .contains("Unsafe"));
        }
        assert_eq!(std::fs::read_to_string(target).unwrap(), "unchanged");
    }
    #[test]
    fn agent_output_is_bounded_and_cannot_mint_approval() {
        let f = Fixture::new();
        let reply = agent_call(
            &f.store,
            "os.mail",
            "propose_reply",
            &json!({"account":"one","message":"message-one","body":"中".repeat(2000)}),
        )
        .unwrap();
        assert_eq!(reply["body_truncated"], true);
        assert!(reply.to_string().len() <= 3800);
        let id = text(&reply, "draft_id");
        let proposal = agent_call(
            &f.store,
            "os.mail",
            "propose_send",
            &json!({"account":"one","draft_id":id,"expected_revision":1}),
        )
        .unwrap();
        assert_eq!(proposal["status"], "awaiting_approval");
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);
        assert!(!proposal.to_string().contains("review-"));
    }
    #[test]
    fn explicit_unknown_retry_requires_acknowledgement_and_an_inactive_last_attempt() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        *f.transport.failure.lock().unwrap() = Some(SendFailure::Unknown("lost response".into()));
        let failed = f.send(f.review(id, 1)).unwrap();
        let prior = text(&failed["attempts"][0], "operation_id");
        assert!(retry_in(&f.store, "os.mail", "one", id, prior, false).is_err());
        assert!(retry_in(&f.store, "os.mail", "one", id, "wrong-attempt", true).is_err());
        let review = retry_in(&f.store, "os.mail", "one", id, prior, true).unwrap();
        assert_eq!(review.snapshot()["prior_status"], "outcome_unknown");
        assert!(
            retry_in(&f.store, "os.mail", "one", id, prior, true).is_err(),
            "a pending retry already exists"
        );
        let sent = f.send(review).unwrap();
        let accepted = text(&sent["attempts"][1], "operation_id");
        assert!(retry_in(&f.store, "os.mail", "one", id, accepted, true).is_err());
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn cancelling_one_review_invalidates_other_reviews_of_that_operation() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        let cancel = f.review(id, 1);
        let other = f.review(id, 1);
        cancel_in(cancel, Arc::new(TestVault)).unwrap();
        assert_eq!(f.send(other).unwrap()["status"], "draft");
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);
        assert_eq!(f.read(id)["attempts"][0]["status"], "cancelled");
    }
    #[test]
    fn simultaneous_review_cannot_resubmit_and_network_wait_does_not_hold_draft_lock() {
        struct WaitingTransport {
            entered: std::sync::mpsc::Sender<()>,
            release: Mutex<std::sync::mpsc::Receiver<()>>,
        }
        impl Transport for WaitingTransport {
            fn test(&self, _: &Value) -> Result<(), String> {
                Ok(())
            }
            fn folders(&self, _: &Value) -> Result<Vec<Value>, String> {
                Ok(vec![])
            }
            fn fetch(&self, _: &Value, _: &str, _: &Value) -> Result<Value, String> {
                Ok(json!({}))
            }
            fn mark_seen(&self, _: &Value, _: &str, _: &Value) -> Result<(), String> {
                Ok(())
            }
            fn send(&self, _: &Value, _: &Value) -> Result<Value, String> {
                self.entered.send(()).unwrap();
                self.release.lock().unwrap().recv().unwrap();
                Ok(json!({"accepted":true}))
            }
        }
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        let first = f.review(id, 1);
        let second = f.review(id, 1);
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let transport = WaitingTransport {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        };
        let task = std::thread::spawn(move || execute(first, &transport, Arc::new(TestVault)));
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        // Separate thread with a deadline makes a lock held during networking
        // a meaningful failure, without leaving the blocked worker behind.
        let (duplicate_tx, duplicate_rx) = std::sync::mpsc::channel();
        let duplicate = std::thread::spawn(move || {
            let fake = Fake::default();
            duplicate_tx
                .send(execute(second, &fake, Arc::new(TestVault)))
                .unwrap();
            assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
        });
        let result = duplicate_rx.recv_timeout(std::time::Duration::from_secs(2));
        release_tx.send(()).unwrap();
        assert_eq!(task.join().unwrap().unwrap()["status"], "accepted");
        duplicate.join().unwrap();
        assert_eq!(result.unwrap().unwrap()["status"], "sending");
    }
    #[test]
    fn composer_returns_host_identity_and_cas_preserves_thread_binding() {
        let f = Fixture::new();
        let first=compose_draft(&f.store,"os.mail",&json!({"account":"one","to":"reply@example.com","subject":"Re: Appointment","body":"Human composed","folder":INBOX,"message":"message-one"})).unwrap();
        assert!(text(&first, "compose_id").starts_with("compose-"));
        assert_eq!(first["in_reply_to"], "<original@example.com>");
        assert_eq!(first["body_origin"], "app");
        let r = f.review(text(&first, "draft_id"), 1);
        let mut changes = json!({"account":"one","compose_id":first["compose_id"],"expected_revision":1,"to":"reply@example.com","subject":"Re: Appointment","body":"Edited composer","folder":INBOX,"message":"message-one"});
        let second = compose_draft(&f.store, "os.mail", &changes).unwrap();
        assert_eq!(second["revision"], 2);
        assert_eq!(second["draft_id"], first["draft_id"]);
        assert!(f.send(r).unwrap_err().contains("revision_conflict"));
        assert!(compose_draft(&f.store, "os.mail", &changes)
            .unwrap_err()
            .contains("revision_conflict"));
        changes["expected_revision"] = json!(2);
        changes["message"] = json!("other-message");
        assert!(compose_draft(&f.store, "os.mail", &changes)
            .unwrap_err()
            .contains("source message"));
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn new_message_composer_does_not_fabricate_reply_headers_or_accept_extra_recipients() {
        let f = Fixture::new();
        let mut args = json!({"account":"one","to":"recipient@example.com","subject":"Hello","body":"New email"});
        let new = compose_draft(&f.store, "os.mail", &args).unwrap();
        assert!(new["source_message"]["message"].is_null());
        assert_eq!(new["in_reply_to"], "");
        assert_eq!(new["references"], "");
        args["cc"] = json!("third@example.com");
        assert!(compose_draft(&f.store, "os.mail", &args).is_err());
        let id = text(&new, "draft_id");
        let accepted = f.send(f.review(id, 1)).unwrap();
        assert_eq!(accepted["status"], "accepted");
        args.as_object_mut().unwrap().remove("cc");
        args["compose_id"] = new["compose_id"].clone();
        args["expected_revision"] = json!(1);
        assert!(compose_draft(&f.store, "os.mail", &args)
            .unwrap_err()
            .contains("submission attempt"));
    }
    #[test]
    fn replacing_one_view_capability_keeps_the_other_review_usable() {
        let f = Fixture::new();
        let d = f.create();
        let id = text(&d, "draft_id");
        let old = f.review(id, 1);
        let replacement = f.review(id, 1);
        revoke_review(old);
        assert_eq!(f.send(replacement).unwrap()["status"], "accepted");
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn a_later_reply_key_creates_a_distinct_bound_draft_without_reusing_sent_content() {
        let f = Fixture::new();
        let first = f.create();
        let accepted = f.send(f.review(text(&first, "draft_id"), 1)).unwrap();
        let next = create(
            &f.store,
            "os.mail",
            "one",
            INBOX,
            "message-one",
            "One more question",
            "follow-up-1",
        )
        .unwrap();
        assert_ne!(next["draft_id"], accepted["draft_id"]);
        assert_eq!(next["source_message"], accepted["source_message"]);
        assert_eq!(next["status"], "draft");
        let duplicate = create(
            &f.store,
            "os.mail",
            "one",
            INBOX,
            "message-one",
            "Do not overwrite",
            "follow-up-1",
        )
        .unwrap();
        assert_eq!(duplicate, next);
        assert!(create(
            &f.store,
            "os.mail",
            "one",
            INBOX,
            "message-one",
            "x",
            "../unsafe"
        )
        .is_err());
    }
    #[test]
    fn delayed_approval_worker_rechecks_host_selection_at_atomic_claim() {
        use std::sync::atomic::AtomicBool;
        let f = Fixture::new();
        let d = f.create();
        let review = f.review(text(&d, "draft_id"), 1);
        let selected = Arc::new(AtomicBool::new(true));
        let watched = selected.clone();
        let watched_root = f.root.clone();
        let previous = claim_guard().lock().unwrap().clone();
        on_claim(Some(Arc::new(move |dir, _, _| {
            if dir == watched_root && !watched.load(Ordering::SeqCst) {
                Err("account changed".into())
            } else {
                Ok(())
            }
        })));
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            release_rx.recv().unwrap();
            let transport = Fake::default();
            let result = execute(review, &transport, Arc::new(TestVault));
            (result, transport.calls.load(Ordering::SeqCst))
        });
        // The UI handed the worker an approved capability, then selection
        // changed before the worker acquired the send claim.
        selected.store(false, Ordering::SeqCst);
        release_tx.send(()).unwrap();
        let (result, calls) = worker.join().unwrap();
        on_claim(previous);
        assert!(result.unwrap_err().contains("account changed"));
        assert_eq!(calls, 0);
        assert_eq!(f.read(text(&d, "draft_id"))["status"], "awaiting_approval");
    }
}
