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

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConsentFile {
    schema: u32,
    apps: BTreeMap<String, Record>,
}

#[derive(Debug)]
pub struct ConsentStore {
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
        ConsentStore { path: None, decided: BTreeMap::new(), known: BTreeMap::new(), asking: Vec::new(), revoked: Vec::new(), allowed: Vec::new(), generation: 0 }
    }
    pub fn in_home(home: &Path) -> ConsentStore {
        let path = home.join(CONSENT_FILE);
        let file: ConsentFile = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        ConsentStore { path: Some(path), decided: file.apps, ..ConsentStore::memory() }
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
