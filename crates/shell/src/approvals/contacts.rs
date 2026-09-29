//! The person's contacts for the rule condition "recipients in my contacts"
//! (ADR 0004 §8), and the person's consent to use them.
//!
//! - [`ContactsSource`] answers whether an email address or Matrix ID is
//!   known. [`ContactList`] is the fixed list (tests, snapshots),
//!   [`NoContacts`] knows nobody, [`MailContacts`] reads Mail's host
//!   service data: the person's own accounts and everyone they sent mail to
//!   (`octosense_mail_service::contacts::known_addresses`), never the
//!   secret store.
//! - [`ContactsGate`] is what the router asks: off by default, it knows
//!   nobody until the person turns on Settings → Assistant → Approvals →
//!   "Use my contacts in approval rules" (a gesture, like consent at first
//!   use); turning it off needs none. The choice is kept per OctoSense home
//!   in [`CONTACTS_FILE`], owner-only.
//!
//! OS address books (macOS Contacts, Android's contacts provider) are a
//! follow-up source behind the same trait.

use super::rules::ApprovalGesture;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Relative to the OctoSense home.
pub const CONTACTS_FILE: &str = "approvals/contacts.json";

/// Who the person knows. `address` is an email address or a Matrix ID
/// (`@ana:example.org`), as the rule facts give it (lowercased).
pub trait ContactsSource: Send {
    fn is_known(&self, address: &str) -> bool;
}

/// Nobody is known.
#[derive(Default)]
pub struct NoContacts;

impl ContactsSource for NoContacts {
    fn is_known(&self, _address: &str) -> bool {
        false
    }
}

/// A fixed list (tests, and a host-provided snapshot).
#[derive(Default, Clone)]
pub struct ContactList(pub Vec<String>);

impl ContactsSource for ContactList {
    fn is_known(&self, address: &str) -> bool {
        let address = address.trim();
        !address.is_empty() && self.0.iter().any(|c| c.trim().eq_ignore_ascii_case(address))
    }
}

/// Mail's host service data, read where the Card runner keeps it
/// (`<apps root>/.host`), re-read at most every [`MailContacts::REFRESH`].
/// `host_dir` answers `None` until App Hub has a data root.
pub struct MailContacts {
    host_dir: Box<dyn Fn() -> Option<PathBuf> + Send>,
    read: fn(&Path) -> Vec<String>,
    cache: std::sync::Mutex<Option<(std::time::Instant, ContactList)>>,
}

impl MailContacts {
    pub const REFRESH: std::time::Duration = std::time::Duration::from_secs(30);

    /// `read` is `octosense_mail_service::contacts::known_addresses` in the
    /// shell; tests hand their own.
    pub fn new(host_dir: impl Fn() -> Option<PathBuf> + Send + 'static, read: fn(&Path) -> Vec<String>) -> MailContacts {
        MailContacts { host_dir: Box::new(host_dir), read, cache: std::sync::Mutex::new(None) }
    }

    fn snapshot(&self) -> ContactList {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, list)) = cache.as_ref() {
            if at.elapsed() < Self::REFRESH {
                return list.clone();
            }
        }
        let list = ContactList((self.host_dir)().map(|d| (self.read)(&d)).unwrap_or_default());
        *cache = Some((std::time::Instant::now(), list.clone()));
        list
    }
}

impl ContactsSource for MailContacts {
    fn is_known(&self, address: &str) -> bool {
        self.snapshot().is_known(address)
    }
}

/// The shell's contacts source: Mail's data where Mail's host service is
/// linked (App Hub, and every phone build), else nobody.
pub fn shell_source() -> Box<dyn ContactsSource> {
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        Box::new(MailContacts::new(
            || octosense_app_hub_app::data_root_if_set().map(|root| root.join(".host")),
            octosense_mail_service::contacts::known_addresses,
        ))
    }
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    {
        Box::new(NoContacts)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ContactsFile {
    schema: u32,
    use_in_rules: bool,
    at: u64,
}

/// The source behind the person's consent: knows nobody while "Use my
/// contacts in approval rules" is off.
pub struct ContactsGate {
    path: Option<PathBuf>,
    allowed: bool,
    source: Box<dyn ContactsSource>,
    generation: u64,
}

impl ContactsGate {
    /// Off, kept in memory only.
    pub fn memory(source: Box<dyn ContactsSource>) -> ContactsGate {
        ContactsGate { path: None, allowed: false, source, generation: 0 }
    }
    /// The person's choice in this home (off if never made).
    pub fn in_home(home: &Path, source: Box<dyn ContactsSource>) -> ContactsGate {
        let path = home.join(CONTACTS_FILE);
        let file: ContactsFile = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        ContactsGate { path: Some(path), allowed: file.use_in_rules, ..ContactsGate::memory(source) }
    }
    pub fn allowed(&self) -> bool {
        self.allowed
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn set_source(&mut self, source: Box<dyn ContactsSource>) {
        self.source = source;
        self.generation += 1;
    }
    /// The person turned "Use my contacts in approval rules" on.
    pub fn allow(&mut self, _gesture: &ApprovalGesture, now: u64) {
        self.set(true, now);
    }
    /// Turning it off needs no gesture (always allowed).
    pub fn turn_off(&mut self, now: u64) {
        self.set(false, now);
    }
    fn set(&mut self, allowed: bool, now: u64) {
        self.allowed = allowed;
        self.generation += 1;
        let Some(path) = &self.path else { return };
        let file = ContactsFile { schema: 1, use_in_rules: allowed, at: now };
        if let Ok(bytes) = serde_json::to_vec_pretty(&file) {
            if let Err(e) = super::write_private(path, &bytes) {
                eprintln!("approvals: could not save the contacts choice: {e}");
            }
        }
    }
}

impl ContactsSource for ContactsGate {
    fn is_known(&self, address: &str) -> bool {
        self.allowed && self.source.is_known(address)
    }
}
