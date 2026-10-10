//! Android execution leases and a private notification outbox. The OS job
//! supplies time, never consent, tools, instructions, or a second agent.
use crate::glance::GlanceCard;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static FOREGROUND: AtomicBool = AtomicBool::new(!cfg!(target_os = "android"));
struct Lease {
    id: u64,
    until: Instant,
    started: u64,
}
static JOB: Mutex<Option<Lease>> = Mutex::new(None);
static NEXT: AtomicU64 = AtomicU64::new(1);
static OUTBOX: Mutex<()> = Mutex::new(());
static RESTORING: AtomicBool = AtomicBool::new(false);
static LAST_RESTORE: Mutex<Option<Instant>> = Mutex::new(None);
const MAX_ENTRIES: usize = 64;
const MAX_BYTES: u64 = 8 * 1024 * 1024;

pub fn foreground(active: bool) {
    FOREGROUND.store(active, Ordering::Release);
}
pub fn begin() -> u64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    *JOB.lock().unwrap() = Some(Lease {
        id,
        until: Instant::now() + Duration::from_secs(240),
        started: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    });
    id
}
pub fn end(id: u64) {
    let mut job = JOB.lock().unwrap();
    if job.as_ref().is_some_and(|current| current.id == id) {
        *job = None;
    }
}
pub fn execution_allowed() -> bool {
    FOREGROUND.load(Ordering::Acquire)
        || JOB
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|lease| Instant::now() < lease.until)
}
/// A fresh job must check once even if foreground polling recently set a due time.
pub fn lease_id() -> Option<u64> {
    JOB.lock()
        .unwrap()
        .as_ref()
        .filter(|lease| Instant::now() < lease.until)
        .map(|lease| lease.id)
}

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    token: String,
    account: String,
    key: String,
    args: Value,
    published: u64,
    expires: u64,
    delivered: bool,
    dismissed: bool,
}
impl Entry {
    fn publisher(&self) -> Option<&str> {
        publisher(&self.key, self.args["card_id"].as_str()?)
    }
    fn visible(&self, account: Option<&str>, now: u64) -> bool {
        !self.dismissed && self.expires > now && Some(self.account.as_str()) == account
    }
}
fn publisher<'a>(key: &'a str, card_id: &str) -> Option<&'a str> {
    let (app, id) = key.split_once('/')?;
    let valid = |value: &str, max| {
        !value.is_empty()
            && value.len() <= max
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    (id == card_id
        && valid(app, 256)
        && app.as_bytes()[0].is_ascii_alphanumeric()
        && valid(id, crate::glance::CARD_ID_MAX))
    .then_some(app)
}
fn token(account: &str, key: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{account}\0{key}").as_bytes())
    )
}
fn persists(app: &str) -> bool {
    // Built-in apps retain their existing dedicated publication stores. Mail's
    // native Android notices keep this store and its original token format.
    !app.starts_with("os.") || (app == "os.mail" && cfg!(target_os = "android"))
}
fn directory() -> Result<std::path::PathBuf, String> {
    let storage = crate::app_storage::host().ok_or("Storage unavailable")?;
    let root = storage.layout().apps_root();
    let dir = root.join(".host/mail-notifications");
    crate::app_storage::ensure_private_dir(root, &dir)
        .map_err(|_| "Notification storage unavailable")?;
    Ok(dir)
}
fn read(dir: &Path) -> Result<Vec<Entry>, String> {
    let path = dir.join("outbox.json");
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("Notification storage unreadable".into()),
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_BYTES {
        return Err("Invalid notification storage".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err("Linked notification storage refused".into());
        }
    }
    let entries: Vec<Entry> =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "Notification read failed")?)
            .map_err(|_| "Invalid notification records")?;
    if entries.len() > MAX_ENTRIES
        || entries.iter().any(|e| {
            e.token != token(&e.account, &e.key)
                || e.publisher().is_none()
                || e.account.is_empty()
                || e.published >= e.expires
                || e.expires - e.published > crate::glance::EXPIRES_MAX_S * 1000
        })
    {
        return Err("Invalid notification scope".into());
    }
    Ok(entries)
}
fn save(dir: &Path, entries: &[Entry]) -> Result<(), String> {
    use std::io::Write;
    let bytes = serde_json::to_vec(entries).map_err(|_| "Notification encoding failed")?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Notification storage full".into());
    }
    let tmp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> std::io::Result<()> {
        let mut file = options.open(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, dir.join("outbox.json"))?;
        #[cfg(unix)]
        std::fs::File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    result.map_err(|_| "Notification persistence failed".into())
}
/// Called only after Glance has validated the caller and its actual publication.
/// The original directory and token format retain existing Mail notifications.
pub fn record(args: &Value, card: &GlanceCard) -> Result<(), String> {
    if !persists(&card.app) || !card.contained {
        return Ok(());
    }
    let Some(account) = card.account.as_ref() else {
        return Ok(());
    };
    let _guard = OUTBOX.lock().unwrap();
    let dir = directory()?;
    let mut entries = read(&dir)?;
    let key = card.key();
    entries.retain(|e| {
        e.expires > crate::glance::now_ms() && !(e.key == key && &e.account == account)
    });
    entries.sort_by_key(|e| e.published);
    while entries.len() >= MAX_ENTRIES {
        entries.remove(0);
    }
    entries.push(Entry {
        token: token(account, &key),
        account: account.clone(),
        key,
        args: args.clone(),
        published: card.published_ms,
        expires: card.expires_ms,
        delivered: args["notify"] != true,
        dismissed: false,
    });
    save(&dir, &entries)
}
pub fn dismiss(key: &str, dismissed: bool) -> Result<(), String> {
    let Some((_, id)) = key.split_once('/') else {
        return Ok(());
    };
    let Some(app) = publisher(key, id) else {
        return Ok(());
    };
    let Some(account) = crate::ai_host::contained::account_of(app) else {
        return Ok(());
    };
    dismiss_for_account(key, &account, dismissed)
}
pub(crate) fn dismiss_for_account(key: &str, account: &str, dismissed: bool) -> Result<(), String> {
    let Some(app) = key.split_once('/').and_then(|(_, id)| publisher(key, id)) else {
        return Ok(());
    };
    if !persists(app) {
        return Ok(());
    }
    let _guard = OUTBOX.lock().unwrap();
    let dir = directory()?;
    let mut entries = read(&dir)?;
    for entry in &mut entries {
        if entry.key == key && entry.account == account {
            entry.dismissed = dismissed;
        }
    }
    save(&dir, &entries)
}
/// Uninstall erases this publisher's saved card contents for every account,
/// including disconnected accounts that are no longer present in OAuth state.
pub fn forget_app(app: &str) -> Result<(), String> {
    let _guard = OUTBOX.lock().unwrap();
    forget_app_in(&directory()?, app)
}
fn forget_app_in(dir: &Path, app: &str) -> Result<(), String> {
    let mut entries = read(dir)?;
    let before = entries.len();
    entries.retain(|entry| entry.publisher() != Some(app));
    if entries.len() != before {
        save(dir, &entries)?;
    }
    Ok(())
}
fn active() -> Vec<Entry> {
    let entries = {
        let _guard = OUTBOX.lock().unwrap();
        directory().and_then(|d| read(&d)).unwrap_or_default()
    };
    let mut accounts = std::collections::HashMap::new();
    entries
        .into_iter()
        .filter(|e| {
            let Some(app) = e.publisher() else {
                return false;
            };
            if !persists(app) {
                return false;
            }
            let account = accounts.entry(app.to_string()).or_insert_with(|| {
                if !publication_access(
                    app,
                    crate::agents::access(app),
                    crate::host_tools::script_apps::admitted(app),
                ) {
                    return None;
                }
                let account = crate::ai_host::contained::account_of(app)?;
                let storage = crate::app_storage::host()?;
                let scope =
                    (account != crate::ai_host::contained::ACCOUNT).then_some(account.as_str());
                if storage.is_signed_out(app, scope) || storage.refused(app, scope).is_some() {
                    return None;
                }
                Some(account)
            });
            e.visible(account.as_deref(), crate::glance::now_ms())
        })
        .collect()
}
fn publication_access(app: &str, agent: crate::agents::Access, granted: bool) -> bool {
    // Foreground app UI can publish with its installed Glance grant before the
    // person ever enables its agent. Restoring that existing card must not ask
    // for, or imply, permission to run an agent. Explicit revocation remains
    // authoritative; the legacy Mail background path still requires consent.
    granted
        && match agent {
            crate::agents::Access::Allowed => true,
            crate::agents::Access::NotAsked => !app.starts_with("os."),
            crate::agents::Access::Off => false,
        }
}
/// Revalidate model source/account on every cold restore. Restoring does not
/// publish a second notification, extend expiry, or reconstruct a lost draft.
fn restore(entry: &Entry) -> bool {
    let Some(app) = entry.publisher() else {
        return false;
    };
    if let Some(card) =
        crate::glance::card(&entry.key).filter(|card| card.published_ms >= entry.published)
    {
        return card.account.as_deref() == Some(entry.account.as_str())
            && card.account_valid()
            && card
                .l0
                .as_ref()
                .and_then(|l| l.mail.as_ref())
                .is_none_or(|b| !crate::mail_card::completed(b));
    }
    crate::glance::restore_notification_for(
        app,
        &entry.args,
        &entry.account,
        entry.published,
        entry.expires,
    )
    .is_ok()
        && crate::glance::card(&entry.key).is_some()
}
/// Restore ordinary app cards on desktop as well as Android. No new notice or
/// event receipt is generated, and original expiry/account are preserved.
pub fn restore_publications() {
    if RESTORING.swap(true, Ordering::AcqRel) {
        return;
    }
    struct Release;
    impl Drop for Release {
        fn drop(&mut self) {
            RESTORING.store(false, Ordering::Release);
        }
    }
    let _release = Release;
    {
        let mut last = LAST_RESTORE.lock().unwrap();
        if last.is_some_and(|at| at.elapsed() < Duration::from_secs(1)) {
            return;
        }
        *last = Some(Instant::now());
    }
    // Glance calls this while drawing; avoid bundle/storage I/O on every frame.
    for entry in active() {
        restore(&entry);
    }
}
fn matches_receipt(
    entry: &Entry,
    app: &str,
    account: &str,
    card_id: &str,
    published: u64,
    now: u64,
) -> bool {
    entry.publisher() == Some(app)
        && entry.args["card_id"] == card_id
        && entry.published == published
        && entry.visible(Some(account), now)
}
/// Event acknowledgement needs a durable card, not just a live store entry.
/// The caller separately verifies the current app/account grant and live card.
pub fn has_publication(app: &str, account: &str, card_id: &str, published: u64) -> bool {
    if !persists(app) {
        return false;
    }
    let _guard = OUTBOX.lock().unwrap();
    directory().and_then(|dir| read(&dir)).is_ok_and(|entries| {
        entries.iter().any(|entry| {
            matches_receipt(
                entry,
                app,
                account,
                card_id,
                published,
                crate::glance::now_ms(),
            )
        })
    })
}
pub fn state() -> Value {
    crate::glance::expire_now();
    let mail = crate::agent_events::background_status();
    let connected = crate::connected_events::background_status();
    let started = JOB.lock().unwrap().as_ref().map(|lease| lease.started);
    let mut state = combined_status(&mail, &connected, started);
    let entries: Vec<_> = active().into_iter().filter(restore).collect();
    state["active"] = json!(entries.iter().map(|e| e.token.clone()).collect::<Vec<_>>());
    state["notifications"] = json!(entries
        .iter()
        .filter(|e| !e.delivered && (e.publisher() != Some("os.mail") || mail["enabled"] == true))
        .map(|e| json!({
            "token":e.token, "published":e.published, "expires":e.expires,
            "kind":if e.publisher() == Some("os.mail") {"mail"} else {"glance"},
            "title":e.args["title"], "summary":crate::glance::note_summary(&e.args)
        }))
        .collect::<Vec<_>>());
    state
}
fn combined_status(mail: &Value, connected: &Value, started: Option<u64>) -> Value {
    let settled = |worker: &Value, since: u64| {
        worker["enabled"] != true
            || (worker["pending"].as_u64() == Some(0)
                && worker["last_poll_at"]
                    .as_u64()
                    .is_some_and(|at| at >= since)
                && worker["busy"] != true)
    };
    // Keep the original Mail fields for diagnostics; job completion is the AND
    // of both workers, never the legacy inbox's empty queue alone.
    let mut state = mail.clone();
    state["enabled"] = json!(mail["enabled"] == true || connected["enabled"] == true);
    state["connected"] = connected.clone();
    state["job_complete"] =
        json!(started.is_some_and(|since| settled(mail, since) && settled(connected, since)));
    state
}
pub fn delivered(id: &str, published: u64) {
    let _guard = OUTBOX.lock().unwrap();
    let Ok(dir) = directory() else { return };
    let Ok(mut entries) = read(&dir) else { return };
    for e in &mut entries {
        if e.token == id && e.published == published {
            e.delivered = true;
        }
    }
    let _ = save(&dir, &entries);
}
/// An external Intent is navigation only: it must match a private, current,
/// account-bound publication. No extra can invoke a tool or approve sending.
pub fn open(id: &str) -> Option<String> {
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let entry = active().into_iter().find(|e| e.token == id)?;
    restore(&entry).then_some(entry.key)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn foreground_cards_restore_before_agent_consent_without_bypassing_revocation() {
        use crate::agents::Access::{Allowed, NotAsked, Off};
        let app = "org.octosense.samples.googlecalendar";
        assert!(publication_access(app, NotAsked, true));
        assert!(publication_access(app, Allowed, true));
        assert!(!publication_access(app, Off, true));
        for consent in [NotAsked, Allowed, Off] {
            assert!(!publication_access(app, consent, false));
        }
        assert!(!publication_access("os.mail", NotAsked, true));
        assert!(!publication_access("os.mail", Off, true));
        assert!(publication_access("os.mail", Allowed, true));
    }
    #[test]
    fn every_authorized_worker_must_settle_before_the_job_finishes() {
        let off = json!({"enabled":false});
        let ready = json!({"enabled":true,"pending":0,"last_poll_at":100});
        let unknown = json!({"enabled":true,"pending":null});
        assert_eq!(combined_status(&off, &unknown, Some(100))["enabled"], true);
        assert_eq!(
            combined_status(&ready, &unknown, Some(100))["job_complete"],
            false
        );
        assert_eq!(
            combined_status(&unknown, &ready, Some(100))["job_complete"],
            false
        );
        assert_eq!(
            combined_status(&ready, &ready, Some(100))["job_complete"],
            true
        );
        assert_eq!(
            combined_status(&off, &ready, Some(100))["job_complete"],
            true
        );
        assert_eq!(
            combined_status(&ready, &off, Some(100))["job_complete"],
            true
        );
        assert_eq!(combined_status(&off, &off, Some(100))["enabled"], false);
        assert_eq!(combined_status(&ready, &ready, None)["job_complete"], false);
        assert_eq!(
            combined_status(&ready, &ready, Some(101))["job_complete"],
            false
        );
        for change in [
            json!({"pending":1}),
            json!({"busy":true}),
            json!({"pending":null}),
        ] {
            let mut outstanding = ready.clone();
            outstanding
                .as_object_mut()
                .unwrap()
                .extend(change.as_object().unwrap().clone());
            assert_eq!(
                combined_status(&ready, &outstanding, Some(100))["job_complete"],
                false
            );
        }
    }
    #[test]
    fn ended_old_lease_cannot_cancel_the_new_worker_lease() {
        let old = begin();
        let current = begin();
        end(old);
        assert_eq!(lease_id(), Some(current));
        end(current);
        assert_eq!(lease_id(), None);
    }
    #[test]
    fn ordinary_app_publications_preserve_publisher_and_account_scope() {
        let dir = std::env::temp_dir().join(format!("glance-notice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let entry = |app: &str, account: &str| Entry {
            token: token(account, &format!("{app}/message")),
            account: account.into(),
            key: format!("{app}/message"),
            args: json!({"card_id":"message"}),
            published: 10,
            expires: 20,
            delivered: false,
            dismissed: false,
        };
        let entries = [
            entry("os.mail", "a"),
            entry("org.example.inbox", "a"),
            entry("org.example.inbox", "b"),
        ];
        save(&dir, &entries).unwrap();
        let restored = read(&dir).unwrap();
        assert_eq!(restored.len(), 3);
        assert_eq!(restored[0].publisher(), Some("os.mail"));
        assert_eq!(restored[1].publisher(), Some("org.example.inbox"));
        assert_ne!(restored[0].token, restored[1].token);
        assert_ne!(restored[1].token, restored[2].token);
        assert!(!restored[1].visible(Some("b"), 15));
        assert!(matches_receipt(
            &restored[1],
            "org.example.inbox",
            "a",
            "message",
            10,
            15
        ));
        for (app, account, card_id, published, now) in [
            ("org.other.inbox", "a", "message", 10, 15),
            ("org.example.inbox", "b", "message", 10, 15),
            ("org.example.inbox", "a", "other", 10, 15),
            ("org.example.inbox", "a", "message", 11, 15),
            ("org.example.inbox", "a", "message", 10, 20),
        ] {
            assert!(!matches_receipt(
                &restored[1],
                app,
                account,
                card_id,
                published,
                now
            ));
        }
        let mut dismissed = restored[1].clone();
        dismissed.dismissed = true;
        assert!(!matches_receipt(
            &dismissed,
            "org.example.inbox",
            "a",
            "message",
            10,
            15
        ));
        for key in [
            "org.example.inbox/other",
            "org.example.inbox/../message",
            "../message",
            "/message",
        ] {
            let mut tampered = restored[1].clone();
            tampered.key = key.into();
            tampered.token = token(&tampered.account, key);
            save(&dir, &[tampered]).unwrap();
            assert!(read(&dir).is_err(), "{key}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn uninstall_erases_all_publisher_accounts_without_touching_other_apps() {
        let dir = std::env::temp_dir().join(format!("notice-uninstall-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let entry = |app: &str, account: &str| Entry {
            token: token(account, &format!("{app}/card")),
            account: account.into(),
            key: format!("{app}/card"),
            args: json!({"card_id":"card", "summary":"Private synthetic card"}),
            published: 10,
            expires: 20,
            delivered: true,
            dismissed: false,
        };
        let entries = [
            entry("org.example.inbox", "connected"),
            entry("org.example.inbox", "disconnected"),
            entry("org.example.inbox2", "connected"),
            entry("os.mail", "connected"),
        ];
        save(&dir, &entries).unwrap();
        forget_app_in(&dir, "org.example.inbox").unwrap();
        let remaining = read(&dir).unwrap();
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].publisher(), Some("org.example.inbox2"));
        assert_eq!(remaining[1].publisher(), Some("os.mail"));
        let before = std::fs::read(dir.join("outbox.json")).unwrap();
        forget_app_in(&dir, "org.example.inbox").unwrap();
        assert_eq!(before, std::fs::read(dir.join("outbox.json")).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn notification_scope_expiry_and_dismissal_are_authoritative() {
        let mut e = Entry {
            token: token("a", "os.mail/card"),
            account: "a".into(),
            key: "os.mail/card".into(),
            args: json!({"card_id":"card"}),
            published: 10,
            expires: 20,
            delivered: false,
            dismissed: false,
        };
        assert!(e.visible(Some("a"), 19));
        assert!(!e.visible(Some("b"), 19));
        assert!(!e.visible(None, 19));
        assert!(!e.visible(Some("a"), 20));
        e.dismissed = true;
        assert!(!e.visible(Some("a"), 19));
        assert_ne!(token("a", &e.key), token("b", &e.key));
    }
    #[test]
    fn outbox_roundtrip_is_private_bounded_and_rejects_tampered_scope() {
        let dir = std::env::temp_dir().join(format!("mail-notice-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let mut e = Entry {
            token: token("a", "os.mail/card"),
            account: "a".into(),
            key: "os.mail/card".into(),
            args: json!({"card_id":"card"}),
            published: 10,
            expires: 20,
            delivered: false,
            dismissed: false,
        };
        save(&dir, &[e.clone()]).unwrap();
        assert_eq!(read(&dir).unwrap()[0].key, e.key);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.join("outbox.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        e.account = "other".into();
        save(&dir, &[e]).unwrap();
        assert!(read(&dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
