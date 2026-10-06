//! Private publication cache. Absolute timestamps and dismissal tombstones
//! survive restart; drafts remain authoritative in the Mail host service.
use super::{account_valid, host_dir, Binding};
use crate::glance::GlanceCard;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

static CACHE: Mutex<()> = Mutex::new(());
static READY: AtomicBool = AtomicBool::new(false);
static RESTORING: AtomicBool = AtomicBool::new(false);
struct Scan {
    root: Option<PathBuf>,
    account: Option<String>,
    scanned: u64,
    generation: u64,
    checked_completion: u64,
}
static SCAN: Mutex<Scan> = Mutex::new(Scan {
    root: None, account: None, scanned: 0, generation: 0, checked_completion: 0,
});
const MAX_BYTES: u64 = 128 * 1024;
const MAX_FILES: usize = 256;
pub(crate) fn publication_guard() -> MutexGuard<'static, ()> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}
pub(crate) fn publication_host_ready() {
    READY.store(true, Ordering::Release);
}

#[derive(Clone, Serialize, Deserialize)]
struct Publication {
    version: u8,
    args: Value,
    binding: Binding,
    published: u64,
    expires: u64,
    #[serde(default)]
    dismissed: bool,
}
impl Publication {
    fn visible(&self, account: Option<&str>, now: u64) -> bool {
        !self.dismissed && self.expires > now && account == Some(self.binding.account.as_str())
    }
    fn matches(&self, card: &GlanceCard) -> bool {
        self.binding.key() == card.key()
            && self.published == card.published_ms
            && self.expires >= card.expires_ms
            && card
                .l0
                .as_ref()
                .and_then(|l| l.mail.as_ref())
                .is_some_and(|b| {
                    b.account == self.binding.account && b.draft_id == self.binding.draft_id
                })
    }
}
struct Cache {
    directory: PathBuf,
}
impl Cache {
    fn at(root: &Path) -> Result<Self, String> {
        if std::fs::symlink_metadata(root).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Mail publication host root cannot be a symlink".into());
        }
        let directory = root.join("mail-cards");
        crate::app_storage::ensure_private_dir(root, &directory).map_err(|e| e.to_string())?;
        Ok(Self { directory })
    }
    fn path(&self, b: &Binding) -> PathBuf {
        use sha2::{Digest, Sha256};
        self.directory.join(format!(
            "{:x}.json",
            Sha256::digest(format!("{}\0{}", b.account, b.key()).as_bytes())
        ))
    }
    fn load(&self, path: &Path) -> Result<Publication, String> {
        let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_BYTES {
            return Err("Invalid publication file".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if meta.nlink() != 1 {
                return Err("Linked publication file refused".into());
            }
        }
        let p: Publication =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|_| "Invalid publication record")?;
        p.binding.validate()?;
        if p.version != 1
            || !p.args.is_object()
            || p.published >= p.expires
            || p.expires - p.published > crate::glance::EXPIRES_MAX_S * 1000
            || p.args["card_id"].as_str() != Some(p.binding.card_id.as_str())
            || self.path(&p.binding) != path
        {
            return Err("Publication identity/timestamps mismatch".into());
        }
        Ok(p)
    }
    fn save(&self, p: &Publication) -> Result<(), String> {
        use std::io::Write;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let target = self.path(&p.binding);
        if std::fs::symlink_metadata(&target)
            .is_ok_and(|m| m.file_type().is_symlink() || !m.is_file())
        {
            return Err("Unsafe publication target".into());
        }
        let bytes = serde_json::to_vec(p).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Publication cache record is too large".into());
        }
        let temp = target.with_extension(format!(
            "{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| -> std::io::Result<()> {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temp, &target)?;
            std::fs::File::open(&self.directory)?.sync_all()
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result.map_err(|e| format!("Cannot persist Mail publication: {e}"))
    }
    fn entries(&self) -> Vec<(PathBuf, Publication)> {
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return vec![];
        };
        entries
            .flatten()
            .take(MAX_FILES)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension()?.to_str()? != "json" {
                    return None;
                }
                Some((path.clone(), self.load(&path).ok()?))
            })
            .collect()
    }
    fn mark(&self, card: &GlanceCard, dismissed: bool) -> Result<bool, String> {
        let Some(binding) = card.l0.as_ref().and_then(|l| l.mail.as_ref()) else {
            return Ok(true);
        };
        let path = self.path(binding);
        if !path.try_exists().map_err(|e| e.to_string())? {
            return Ok(dismissed);
        }
        let mut p = self.load(&path)?;
        if !p.matches(card) {
            return Ok(false);
        }
        p.dismissed = dismissed;
        self.save(&p)?;
        Ok(true)
    }
    fn remove(&self, path: &Path) -> Result<(), String> {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        std::fs::File::open(&self.directory)
            .and_then(|d| d.sync_all())
            .map_err(|e| e.to_string())
    }
}

/// Caller holds the publication gate across live-store mutation and this save.
pub(crate) fn save_publication(
    _guard: &MutexGuard<'static, ()>,
    args: &Value,
    binding: &Binding,
    published: u64,
    expires: u64,
) -> Result<(), String> {
    let mut args = args.clone();
    args.as_object_mut()
        .ok_or("Publication arguments must be an object")?
        .remove("mail_binding");
    args["notify"] = Value::Bool(false);
    Cache::at(&host_dir()?)?.save(&Publication {
        version: 1,
        args,
        binding: binding.clone(),
        published,
        expires,
        dismissed: false,
    })
}
pub(crate) fn set_publication_dismissed(
    _guard: &MutexGuard<'static, ()>,
    card: &GlanceCard,
    dismissed: bool,
) -> Result<bool, String> {
    if card.l0.as_ref().and_then(|l| l.mail.as_ref()).is_none() {
        return Ok(true);
    }
    Cache::at(&host_dir()?)?.mark(card, dismissed)
}
pub(crate) fn publication_can_undo(card: &GlanceCard, now: u64) -> bool {
    let Some(b) = card.l0.as_ref().and_then(|l| l.mail.as_ref()) else {
        return true;
    };
    if !account_valid(&b.account) || !super::read(b).is_ok_and(|snapshot| !super::reply_completed(&snapshot)) {
        return false;
    }
    let Ok(cache) = host_dir().and_then(|root| Cache::at(&root)) else {
        return false;
    };
    cache
        .load(&cache.path(b))
        .is_ok_and(|p| p.dismissed && p.expires > now && p.matches(card))
}
pub(crate) fn remove_publication(
    _guard: &MutexGuard<'static, ()>,
    key: &str,
) -> Result<(), String> {
    let Ok(root) = host_dir() else { return Ok(()) };
    let cache = Cache::at(&root)?;
    for (_, mut p) in cache.entries() {
        if p.binding.key() == key && account_valid(&p.binding.account) {
            p.dismissed = true;
            cache.save(&p)?;
        }
    }
    Ok(())
}

/// Called by Glance after host services have finished registering, never from
/// the registration Once. Retry readiness/account changes without extending TTL.
pub(crate) fn restore_publications() {
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        if !READY.load(Ordering::Acquire) || RESTORING.swap(true, Ordering::AcqRel) {
            return;
        }
        struct Release;
        impl Drop for Release {
            fn drop(&mut self) {
                RESTORING.store(false, Ordering::Release);
            }
        }
        let _release = Release;
        let Ok(root) = host_dir() else { return };
        // Missing/corrupt account metadata is not evidence of removal.
        let Ok(bytes) = std::fs::read(root.join("mail/accounts.json")) else {
            return;
        };
        let Ok(accounts) = serde_json::from_slice::<Vec<Value>>(&bytes) else {
            return;
        };
        let active = crate::ai_host::contained::account_of("os.mail").filter(|a| account_valid(a));
        let now = crate::glance::now_ms();
        let Ok(_guard) = CACHE.try_lock() else { return };
        let Ok(cache) = Cache::at(&root) else { return };
        let check_completion = {
            let mut scan = SCAN.lock().unwrap_or_else(|e| e.into_inner());
            let generation = super::generation();
            let changed = scan.root.as_ref() != Some(&root) || scan.account != active || scan.generation != generation;
            // Retry transient read failures even without another draft edit,
            // while avoiding draft I/O on every Glance frame/outbox poll.
            let check = changed || now.saturating_sub(scan.checked_completion) >= 30_000;
            if now.saturating_sub(scan.scanned) < 1000 && !check {
                return;
            }
            if check { scan.checked_completion = now; }
            scan.root = Some(root.clone()); scan.account = active.clone();
            scan.scanned = now; scan.generation = generation;
            check
        };
        crate::glance::hide_other_mail_accounts(active.as_deref());
        let mut entries = cache.entries();
        entries.sort_by_key(|(_, p)| std::cmp::Reverse(p.published));
        for (path, mut p) in entries {
            let granted = accounts.iter().any(|a| {
                a["id"].as_str() == Some(p.binding.account.as_str())
                    && a["apps"]
                        .as_array()
                        .is_some_and(|apps| apps.iter().any(|a| a == "os.mail"))
            });
            if p.expires <= now || !granted {
                let _ = cache.remove(&path);
                continue;
            }
            if !p.visible(active.as_deref(), now) {
                continue;
            }
            let live = crate::glance::card(&p.binding.key()).is_some();
            if live && !check_completion {
                continue;
            }
            // A removed/corrupt draft cannot be reconstructed from cached data.
            let Ok(snapshot) = super::read(&p.binding) else { continue; };
            if super::reply_completed(&snapshot) {
                p.dismissed = true;
                if cache.save(&p).is_err() {
                    makepad_widgets::log!("Mail completion: publication cache update failed");
                }
                if crate::mail_background::dismiss_for_account(&p.binding.key(), &p.binding.account, true).is_err() {
                    makepad_widgets::log!("Mail completion: notification cache update failed");
                }
                crate::glance::retire_completed_mail(&_guard, &p.binding);
                continue;
            }
            if live { continue; }
            if let Err(error) =
                crate::glance::restore_mail_publication(&p.args, p.binding, p.published, p.expires)
            {
                makepad_widgets::log!("Mail card restore unavailable: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "mail-cache-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&p);
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn publication(account: &str) -> Publication {
        Publication {
            version: 1,
            args: json!({"card_id":"same-card","source":"view root TextBody(text: \\\"Hi\\\")","data":{},"title":"Mail","notify":false}),
            binding: Binding {
                publisher: "os.mail".into(),
                account: account.into(),
                source_message: json!({"folder":"INBOX","message":"m1"}),
                draft_id: "draft_1".into(),
                draft_revision: 1,
                chat_thread: "thread_1".into(),
                card_id: "same-card".into(),
            },
            published: 1000,
            expires: 61000,
            dismissed: false,
        }
    }
    #[test]
    fn expiry_is_absolute_and_a_tombstone_survives_reopening() {
        let s = Scratch::new();
        let cache = Cache::at(&s.0).unwrap();
        let mut p = publication("one");
        cache.save(&p).unwrap();
        let reopened = Cache::at(&s.0).unwrap();
        let kept = reopened.load(&reopened.path(&p.binding)).unwrap();
        assert!(kept.visible(Some("one"), 60999));
        assert!(!kept.visible(Some("one"), 61000));
        assert_eq!(kept.expires, 61000);
        p.dismissed = true;
        cache.save(&p).unwrap();
        assert!(!reopened
            .load(&reopened.path(&p.binding))
            .unwrap()
            .visible(Some("one"), 2000));
    }
    #[test]
    fn equal_card_ids_are_account_scoped_and_never_retarget() {
        let s = Scratch::new();
        let cache = Cache::at(&s.0).unwrap();
        let one = publication("one");
        let two = publication("two");
        cache.save(&one).unwrap();
        cache.save(&two).unwrap();
        assert_ne!(cache.path(&one.binding), cache.path(&two.binding));
        assert_eq!(cache.entries().len(), 2);
        assert!(!one.visible(Some("two"), 2000));
        assert!(two.visible(Some("two"), 2000));
        cache.remove(&cache.path(&one.binding)).unwrap();
        assert_eq!(cache.entries().len(), 1);
        assert!(cache.load(&cache.path(&two.binding)).is_ok());
    }
    #[test]
    fn corrupt_identity_and_expiry_are_not_restored() {
        let s = Scratch::new();
        let cache = Cache::at(&s.0).unwrap();
        let mut p = publication("one");
        p.expires = p.published;
        cache.save(&p).unwrap();
        assert!(cache.load(&cache.path(&p.binding)).is_err());
        p = publication("one");
        cache.save(&p).unwrap();
        let wrong = cache.path(&publication("two").binding);
        std::fs::rename(cache.path(&p.binding), &wrong).unwrap();
        assert!(cache.load(&wrong).is_err());
    }
    fn card(p: &Publication) -> GlanceCard {
        GlanceCard {
            account: Some(p.binding.account.clone()),
            app: "os.mail".into(),
            card_id: p.binding.card_id.clone(),
            title: "Mail".into(), summary: String::new(),
            priority: 50,
            published_ms: p.published,
            expires_ms: p.expires,
            open_app: "mail".into(),
            route: None,
            body: "".into(),
            contained: true,
            digests: vec![],
            l0: Some(std::sync::Arc::new(crate::glance::L0Source {
                source: "".into(),
                data: serde_json::json!({}),
                mail: Some(p.binding.clone()),
            })),
        }
    }
    #[test]
    fn dismiss_undo_is_version_bound_and_cannot_resurrect_a_republished_card() {
        let scratch = Scratch::new();
        let cache = Cache::at(&scratch.0).unwrap();
        let mut p = publication("one");
        cache.save(&p).unwrap();
        let original = card(&p);
        assert!(cache.mark(&original, true).unwrap());
        assert!(!cache
            .load(&cache.path(&p.binding))
            .unwrap()
            .visible(Some("one"), 2000));
        assert!(cache.mark(&original, false).unwrap());
        assert!(cache
            .load(&cache.path(&p.binding))
            .unwrap()
            .visible(Some("one"), 2000));
        p.published += 1;
        p.expires += 1;
        cache.save(&p).unwrap();
        assert!(!cache.mark(&original, true).unwrap());
        assert!(!cache.mark(&original, false).unwrap());
        assert!(!cache.load(&cache.path(&p.binding)).unwrap().dismissed);
        cache.remove(&cache.path(&p.binding)).unwrap();
        assert!(!cache.mark(&original, false).unwrap());
    }
    #[cfg(unix)]
    #[test]
    fn cache_files_are_private_and_links_are_refused() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let s = Scratch::new();
        let cache = Cache::at(&s.0).unwrap();
        let p = publication("one");
        cache.save(&p).unwrap();
        let file = cache.path(&p.binding);
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let outside = s.0.join("outside");
        std::fs::write(&outside, "unchanged").unwrap();
        std::fs::remove_file(&file).unwrap();
        symlink(&outside, &file).unwrap();
        assert!(cache.load(&file).is_err());
        assert!(cache.save(&p).is_err());
        assert_eq!(std::fs::read_to_string(outside).unwrap(), "unchanged");
    }
}
