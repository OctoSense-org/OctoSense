//! Android execution leases and a private notification outbox. The OS job
//! supplies time, never consent, tools, instructions, or a second agent.
use crate::glance::GlanceCard;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static FOREGROUND: AtomicBool = AtomicBool::new(!cfg!(target_os = "android"));
static JOB: Mutex<Option<(u64, Instant)>> = Mutex::new(None);
static NEXT: AtomicU64 = AtomicU64::new(1);
static OUTBOX: Mutex<()> = Mutex::new(());
const MAX_ENTRIES: usize = 64;
const MAX_BYTES: u64 = 8 * 1024 * 1024;

pub fn foreground(active: bool) {
    FOREGROUND.store(active, Ordering::Release);
}
pub fn begin() -> u64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    *JOB.lock().unwrap() = Some((id, Instant::now() + Duration::from_secs(240)));
    id
}
pub fn end(id: u64) {
    let mut job = JOB.lock().unwrap();
    if job.as_ref().is_some_and(|(current, _)| *current == id) {
        *job = None;
    }
}
pub fn execution_allowed() -> bool {
    FOREGROUND.load(Ordering::Acquire)
        || JOB
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|(_, until)| Instant::now() < *until)
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
    fn visible(&self, account: Option<&str>, now: u64) -> bool {
        !self.dismissed && self.expires > now && Some(self.account.as_str()) == account
    }
}
fn token(account: &str, key: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{account}\0{key}").as_bytes())
    )
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
                || e.key != format!("os.mail/{}", e.args["card_id"].as_str().unwrap_or_default())
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
        std::fs::rename(&tmp, dir.join("outbox.json"))?;
        std::fs::File::open(dir)?.sync_all()
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    result.map_err(|_| "Notification persistence failed".into())
}
/// Called only after Glance has validated an actual Mail publication.
pub fn record(args: &Value, card: &GlanceCard) -> Result<(), String> {
    if !cfg!(target_os = "android") || card.app != "os.mail" {
        return Ok(());
    }
    let account = card
        .account
        .as_ref()
        .ok_or("Mail notification needs its account")?;
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
    if !cfg!(target_os = "android") || !key.starts_with("os.mail/") {
        return Ok(());
    }
    let account = crate::ai_host::contained::account_of("os.mail");
    let _guard = OUTBOX.lock().unwrap();
    let dir = directory()?;
    let mut entries = read(&dir)?;
    for entry in &mut entries {
        if entry.key == key && Some(&entry.account) == account.as_ref() {
            entry.dismissed = dismissed;
        }
    }
    save(&dir, &entries)
}
fn active() -> Vec<Entry> {
    let account = crate::ai_host::contained::account_of("os.mail");
    if crate::agents::access("os.mail") != crate::agents::Access::Allowed
        || account.as_ref().is_none_or(|a| {
            crate::app_storage::host().is_none_or(|s| s.is_signed_out("os.mail", Some(a)))
        })
    {
        return Vec::new();
    }
    let _guard = OUTBOX.lock().unwrap();
    directory()
        .and_then(|d| read(&d))
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.visible(account.as_deref(), crate::glance::now_ms()))
        .collect()
}
/// Revalidate model source/account on every cold restore. Restoring does not
/// publish a second notification, extend expiry, or reconstruct a lost draft.
fn restore(entry: &Entry) -> bool {
    if crate::glance::card(&entry.key).is_some_and(|card| card.published_ms >= entry.published) {
        return true;
    }
    crate::glance::restore_mail_notification(
        &entry.args,
        &entry.account,
        entry.published,
        entry.expires,
    )
    .is_ok()
        && crate::glance::card(&entry.key).is_some()
}
pub fn state() -> Value {
    let mut state = crate::agent_events::background_status();
    let entries: Vec<_> = active().into_iter().filter(restore).collect();
    state["active"] = json!(entries.iter().map(|e| e.token.clone()).collect::<Vec<_>>());
    state["notifications"] = json!(entries
        .iter()
        .filter(|e| state["enabled"] == true && !e.delivered)
        .map(|e| json!({
            "token":e.token, "published":e.published, "expires":e.expires,
            "title":e.args["title"], "summary":crate::glance::note_summary(&e.args)
        }))
        .collect::<Vec<_>>());
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
