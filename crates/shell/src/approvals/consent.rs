//! Consent at first use (ADR 0004 §4): the first time an app asks for its
//! agent, the shell shows what the agent may read and use (from its manifest
//! and grants) and where the model runs; the person allows or denies, and
//! the answer is remembered per OctoSense home. Settings lists every app's
//! agent with an off switch. Developer mode asks nothing (§13).
//!
//! [`granted`] is what #106's contained apps (`Policy::contained_apps` per
//! app) and the Rinx/native offer path ask before handing an app its peer.

use super::rules::ApprovalGesture;
use crate::app_storage::{AgentWorkspace, AppKind, StorageSpec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Relative to the OctoSense home.
pub const CONSENT_FILE: &str = "approvals/consent.json";

/// What the first-use sheet shows about one app's agent.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AgentSummary {
    pub app: String,
    pub name: String,
    /// What it may read ("Your mail in this account's folder").
    pub reads: Vec<String>,
    /// What it may use (its own tools, granted tools of other apps,
    /// toolbox tools, command execution).
    pub uses: Vec<String>,
    /// Where the model runs ("OpenAI (api.openai.com), set in AI providers").
    pub model: String,
}

impl AgentSummary {
    /// From a script app's `manifest.json` (its `capabilities` and
    /// `storage`) or a `native-apps.json` entry (`agent` and `storage`),
    /// with the grants the person gave at install and the model's place.
    pub fn from_manifest(app: &str, name: &str, manifest: &Value, granted: &[String], model: &str) -> AgentSummary {
        let mut reads = Vec::new();
        // The storage block as app storage reads it, so the sheet and the
        // folders agree (a block the store refuses is shown as no block;
        // the install check reports it). `external` is native-only and is
        // never an agent's workspace, so parse as native.
        let storage = StorageSpec::from_manifest(manifest, AppKind::Native).unwrap_or_default();
        match storage.agent_workspace {
            AgentWorkspace::None => reads.push("No files: only what its tools return".to_string()),
            AgentWorkspace::Account if storage.accounts => reads.push(format!("{name}'s files for the signed-in account")),
            AgentWorkspace::Account => reads.push(format!("{name}'s files on this device")),
        }
        reads.push("Its own memory".to_string());
        let mut declared: Vec<String> = Vec::new();
        if let Some(caps) = manifest.get("capabilities").and_then(|v| v.as_array()) {
            declared.extend(caps.iter().filter_map(|c| c.as_str()).map(str::to_string));
        }
        if let Some(octos) = manifest.get("agent").and_then(|a| a.get("octos")).and_then(|v| v.as_array()) {
            declared.extend(octos.iter().filter_map(|c| c.as_str()).map(str::to_string));
        }
        let mut uses: Vec<String> = declared.iter().filter(|d| granted.iter().any(|g| g == *d)).map(|d| describe_capability(d)).collect();
        uses.dedup();
        if uses.is_empty() {
            uses.push(format!("{name}'s own tools"));
        }
        // A script app's agent that keeps App Hub's one kernel tool for
        // contained apps (`ask_user_question`), in the store's words.
        if manifest["agent"]["tools"].as_array().is_some_and(|t| t.iter().any(|t| t == "ask_user_question")) {
            uses.push("Ask you questions".to_string());
        }
        AgentSummary { app: app.into(), name: name.into(), reads, uses, model: model.into() }
    }
}

fn describe_capability(cap: &str) -> String {
    match cap {
        "research" => "Web search and page reading (the system toolbox)".into(),
        "crawl" => "Crawling websites (the system toolbox)".into(),
        "model" => "One-shot model calls".into(),
        "command" | "commands" => "Running commands, each approved by you".into(),
        c if c.starts_with("octos.") => format!("Its agent ({c})"),
        c if c.contains('.') => format!("Another app's tool: {c}"),
        c => c.to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Undecided,
    Allowed,
    Denied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    allowed: bool,
    at: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ConsentFile {
    schema: u32,
    apps: BTreeMap<String, Record>,
}

#[derive(Debug)]
pub struct ConsentStore {
    writer: Option<ConsentWriter>,
    path: Option<PathBuf>,
    decided: BTreeMap<String, Record>,
    /// Every app with an agent the shell knows of (for Settings).
    known: BTreeMap<String, AgentSummary>,
    /// First-use prompts waiting for the person, oldest first.
    asking: Vec<String>,
    /// Apps whose agent was just turned off: the shell revokes their live
    /// services ([`ConsentStore::take_revoked`]).
    revoked: Vec<String>,
    /// Apps whose agent was just allowed: the shell prepares their peer
    /// ([`ConsentStore::take_allowed`], `crate::agents`).
    allowed: Vec<String>,
    generation: u64,
}

impl ConsentStore {
    pub fn memory() -> ConsentStore {
        ConsentStore { writer: None, path: None, decided: BTreeMap::new(), known: BTreeMap::new(), asking: Vec::new(), revoked: Vec::new(), allowed: Vec::new(), generation: 0 }
    }
    pub fn in_home(home: &Path) -> ConsentStore {
        let path = home.join(CONSENT_FILE);
        let file: ConsentFile = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        ConsentStore { path: Some(path), decided: file.apps, ..ConsentStore::memory() }
    }
    pub(crate) fn enable_writer(&mut self, pool: makepad_widgets::makepad_platform::thread::TaskPool) {
        if self.writer.is_none() {
            if let Some(path) = &self.path { self.writer = Some(ConsentWriter::new(path.clone(), pool)); }
        }
    }
    pub(crate) fn persistence_busy(&self) -> bool {
        self.writer.as_ref().is_some_and(|w| {
            w.start();
            w.shared.running.load(std::sync::atomic::Ordering::Acquire)
                || !w.shared.pending.is_empty() || !w.shared.errors.is_empty()
        })
    }
    pub(crate) fn persistence_errors(&self) -> Vec<String> {
        let Some(writer) = &self.writer else { return Vec::new() };
        writer.start();
        let mut errors = Vec::new();
        while let Some(error) = writer.shared.errors.pop() { errors.push(error); }
        errors
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn state(&self, app: &str) -> State {
        match self.decided.get(app) {
            None => State::Undecided,
            Some(r) if r.allowed => State::Allowed,
            Some(_) => State::Denied,
        }
    }
    /// Whether the app may have its agent now. `grants_all`: developer mode.
    pub fn granted(&self, app: &str, grants_all: bool) -> bool {
        grants_all || self.state(app) == State::Allowed
    }
    /// The shell learns of an app with an agent (Settings lists it).
    pub fn register(&mut self, summary: AgentSummary) {
        if self.known.get(&summary.app) != Some(&summary) {
            self.known.insert(summary.app.clone(), summary);
            self.generation += 1;
        }
    }
    /// An app asks for its agent. Undecided: the first-use sheet is queued
    /// (once) and the answer is `Undecided` until the person chooses.
    pub fn ask(&mut self, summary: AgentSummary, grants_all: bool) -> State {
        let app = summary.app.clone();
        self.register(summary);
        if grants_all {
            return State::Allowed;
        }
        let state = self.state(&app);
        if state == State::Undecided && !self.asking.contains(&app) {
            self.asking.push(app);
            self.generation += 1;
        }
        state
    }
    /// The first-use sheet in front.
    pub fn prompt(&self) -> Option<&AgentSummary> {
        self.asking.first().and_then(|a| self.known.get(a))
    }
    /// The person chose, on the first-use sheet or Settings' switch.
    pub fn set(&mut self, _gesture: &ApprovalGesture, app: &str, allowed: bool, now: u64) {
        if !allowed {
            self.revoke(app);
        } else if !self.allowed.iter().any(|a| a == app) {
            self.allowed.push(app.to_string());
        }
        self.decided.insert(app.to_string(), Record { allowed, at: now });
        self.asking.retain(|a| a != app);
        self.generation += 1;
        self.save();
    }
    /// Turning an agent off needs no gesture (always allowed).
    pub fn turn_off(&mut self, app: &str, now: u64) {
        self.revoke(app);
        self.decided.insert(app.to_string(), Record { allowed: false, at: now });
        self.asking.retain(|a| a != app);
        self.generation += 1;
        self.save();
    }
    /// The apps whose agent was allowed since the last call: the shell
    /// prepares their peer now (ADR 0004 §4).
    pub fn take_allowed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.allowed)
    }
    fn revoke(&mut self, app: &str) {
        self.allowed.retain(|a| a != app);
        if !self.revoked.iter().any(|a| a == app) {
            self.revoked.push(app.to_string());
        }
    }
    /// The apps whose agent was turned off since the last call: the shell
    /// closes their live services (peer links and contexts, an in-process
    /// module's service, a contained app's peer) and withdraws the offer.
    pub fn take_revoked(&mut self) -> Vec<String> {
        std::mem::take(&mut self.revoked)
    }
    /// Settings: every app's agent the shell knows of or has an answer for.
    pub fn agents(&self) -> Vec<(String, String, State)> {
        let mut apps: Vec<String> = self.known.keys().cloned().collect();
        for a in self.decided.keys() {
            if !apps.contains(a) {
                apps.push(a.clone());
            }
        }
        apps.sort();
        apps.into_iter()
            .map(|a| {
                let name = self.known.get(&a).map(|s| s.name.clone()).unwrap_or_else(|| super::sheet::app_label(&a));
                let state = self.state(&a);
                (a, name, state)
            })
            .collect()
    }
    fn save(&self) {
        let Some(path) = &self.path else { return };
        let file = ConsentFile { schema: 1, apps: self.decided.clone() };
        if let Some(writer) = &self.writer {
            writer.shared.pending.force_push(file);
            writer.start();
            return;
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&file) {
            if let Err(e) = super::write_private(path, &bytes) {
                eprintln!("approvals: could not save consent: {e}");
            }
        }
    }
}

/// Whether `app` may have its agent now: the person allowed it, or
/// developer mode grants everything. False before the shell sets up.
pub fn granted(app: &str) -> bool {
    super::consent_granted(app)
}

/// An app asks for its agent; the first-use sheet shows if undecided.
pub fn ask(summary: AgentSummary) -> State {
    super::consent_ask(summary)
}

/// One ordered writer per consent file. A newer complete snapshot supersedes a
/// queued one; the worker never holds Approvals' mutex during disk I/O. The UI
/// retries pool admission on its maintenance tick and surfaces write failures.
#[derive(Debug)]
struct ConsentWriter {
    pool: makepad_widgets::makepad_platform::thread::TaskPool,
    shared: std::sync::Arc<ConsentWrites>,
}
#[derive(Debug)]
struct ConsentWrites {
    path: PathBuf,
    pending: crossbeam_queue::ArrayQueue<ConsentFile>,
    errors: crossbeam_queue::ArrayQueue<String>,
    running: std::sync::atomic::AtomicBool,
}
impl ConsentWriter {
    fn new(path: PathBuf, pool: makepad_widgets::makepad_platform::thread::TaskPool) -> Self {
        Self { pool, shared: std::sync::Arc::new(ConsentWrites {
            path, pending: crossbeam_queue::ArrayQueue::new(1),
            errors: crossbeam_queue::ArrayQueue::new(4), running: false.into(),
        }) }
    }
    fn start(&self) {
        use std::sync::atomic::Ordering;
        use makepad_widgets::makepad_platform::thread::Lane;
        if self.shared.pending.is_empty() || self.shared.running.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() { return; }
        let shared = self.shared.clone();
        match self.pool.submit_named(Lane::Heavy, "save agent consent", move || shared.drain()) {
            Ok(task) => task.detach(),
            Err(error) => {
                if error == makepad_widgets::makepad_platform::thread::SubmitError::Closed {
                    self.shared.pending.pop();
                    self.shared.errors.force_push("Your agent choice could not be saved: background writer unavailable".into());
                }
                self.shared.running.store(false, Ordering::Release);
            }
        }
    }
}
impl ConsentWrites {
    fn drain(&self) {
        use std::sync::atomic::Ordering;
        loop {
            while let Some(file) = self.pending.pop() {
                let result = serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())
                    .and_then(|bytes| super::write_private(&self.path, &bytes).map_err(|e| e.to_string()));
                if let Err(e) = result {
                    self.errors.force_push(format!("Your agent choice could not be saved: {e}"));
                }
            }
            self.running.store(false, Ordering::Release);
            // A producer can publish just before we relinquish the writer. It
            // either starts the successor or this worker picks up that snapshot.
            if self.pending.is_empty() || self.running.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() { break; }
        }
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use makepad_widgets::makepad_platform::thread::TaskPool;

    fn fixture() -> (ConsentStore, PathBuf) {
        let root = std::env::temp_dir().join(format!("consent-writer-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let path = root.join(CONSENT_FILE);
        let store = ConsentStore { path: Some(path), ..ConsentStore::memory() };
        (store, root)
    }

    #[test]
    fn queued_revocation_supersedes_allow_and_remains_private() {
        let (mut store, root) = fixture();
        let writer = ConsentWriter::new(root.join(CONSENT_FILE), TaskPool::closed());
        store.path = None;
        super::super::tests::allow_consent_for_test(&mut store, "os.mail", 1);
        writer.shared.pending.force_push(ConsentFile { schema: 1, apps: store.decided.clone() });
        store.turn_off("os.mail", 2);
        writer.shared.pending.force_push(ConsentFile { schema: 1, apps: store.decided.clone() });
        assert!(!store.granted("os.mail", false));
        assert!(store.take_allowed().is_empty());
        assert_eq!(store.take_revoked(), ["os.mail"]);
        assert!(!root.join(CONSENT_FILE).exists(), "UI must not write to disk");
        writer.shared.drain();
        let restored = ConsentStore::in_home(&root);
        assert_eq!(restored.state("os.mail"), State::Denied);
        assert_eq!(restored.decided["os.mail"].at, 2);
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(root.join(CONSENT_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_write_is_visible_and_does_not_undo_current_revocation() {
        let (mut store, root) = fixture();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("approvals"), "not a directory").unwrap();
        store.path = None;
        store.turn_off("os.mail", 3);
        let writer = ConsentWriter::new(root.join(CONSENT_FILE), TaskPool::closed());
        writer.shared.pending.force_push(ConsentFile { schema: 1, apps: store.decided.clone() });
        writer.shared.drain();
        store.writer = Some(writer);
        assert!(!store.granted("os.mail", false));
        assert!(store.persistence_busy(), "quit must let the UI report the error");
        assert_eq!(store.persistence_errors().len(), 1);
        assert!(store.persistence_errors().is_empty());
        assert!(!store.persistence_busy());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unavailable_writer_reports_failure_instead_of_holding_quit_forever() {
        let (mut store, root) = fixture();
        store.enable_writer(TaskPool::closed());
        store.turn_off("os.mail", 4);
        assert!(store.persistence_busy());
        assert_eq!(store.persistence_errors().len(), 1);
        assert!(!store.persistence_busy());
        assert!(!root.join(CONSENT_FILE).exists());
    }

}
