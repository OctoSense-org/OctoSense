//! The two containers, derived from one policy.
//!
//! [`IsolateSettings`] is what the widget that mounts the app applies to its
//! splash isolate at creation — the same four knobs the isolate already has,
//! plus the two budgets ADR 0002 phase 4 adds. [`SessionProfile`] is what the
//! kernel is asked for when the app wants an agent. Both come from the same
//! [`AppPolicy`], which is the point: one declaration, two containers, no way
//! for them to disagree.
use crate::policy::AppPolicy;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// What the host applies to the app's splash isolate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IsolateSettings {
    /// Manifest declarations. Use public_runtime_capabilities when applying
    /// runtime availability; host services independently check real consent.
    pub capabilities: Vec<String>,
    /// The isolate's own network module, available to every admitted app.
    /// Its host list is a declaration since makepad#117: the runtime does not
    /// hold the module to it (OctoSense #450).
    pub allow_net: bool,
    /// Whole-jail ceiling in bytes.
    pub storage_quota: u64,
    /// May this isolate's surface raise a prompt: a service's sheet over the
    /// app. The surface decides, not the manifest: true for an app in the
    /// foreground, which is what these settings describe; a host running the
    /// app in the background (a home-screen tile) sets it false.
    pub host_prompts: bool,
    /// Cumulative script instructions for the app's session.
    pub instruction_budget: u64,
    /// Heap ceiling for the isolate.
    pub memory_bytes: u64,
    /// The jail's directory, under the host's app-data root.
    pub jail_root: PathBuf,
    /// Declared destinations for disclosure; not a network access gate.
    pub hosts: Vec<String>,
}

/// Runtime-owned public flags, separate from manifest disclosure. Device
/// reads require a host with the per-app consent broker. Internal `profile`
/// and unowned `agent.notify` are deliberately absent: installed apps use
/// authenticated host requests and reviewed app tools instead.
pub fn public_runtime_capabilities(declared: &[String], device_consent: bool, explicit_capture_intent: bool) -> Vec<String> {
    let device = ["camera", "microphone", "location", "library"];
    let mut capabilities: std::collections::BTreeSet<String> = declared.iter()
        .filter(|name| !device.contains(&name.as_str()) && !["profile", "agent"].contains(&name.as_str()))
        .cloned().collect();
    capabilities.extend(["storage", "net", "images", "web", "prompt"].map(str::to_string));
    if device_consent {
        capabilities.extend(["camera", "location"].map(str::to_string));
        // Read from the host's registered camera.capture_intent@1 ABI, never
        // from the app's manifest. Older CameraPreview interprets these
        // flags as automatic recording/export, so mixed builds keep them off.
        if explicit_capture_intent {
            capabilities.extend(["microphone", "library"].map(str::to_string));
        }
    }
    capabilities.into_iter().collect()
}

impl IsolateSettings {
    /// Every admitted app receives its own `fs` jail. The `storage`
    /// declaration describes expected use; it cannot remove isolation.
    pub fn storage_root(&self) -> Option<PathBuf> {
        Some(self.jail_root.clone())
    }

    /// The resolved quota applies even when storage usage is undeclared.
    pub fn granted_storage_quota(&self) -> u64 {
        self.storage_quota
    }
}

/// What the kernel is asked for: a session that can reach no more than the
/// app itself can. Serialises into the shape the kernel's profile takes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionProfile {
    /// `<app id>.agent`, so a session is traceable to the app in every log.
    pub session_id: String,
    /// The permission profile mode, as the kernel names it.
    pub mode: &'static str,
    /// The only directory the session may write: the app's own jail.
    pub workspace: PathBuf,
    /// Nothing outside the workspace is readable unless listed here, and
    /// nothing is listed here today.
    pub read_allow_paths: Vec<PathBuf>,
    pub tools: Vec<String>,
    pub hosts: Vec<String>,
    pub max_iterations: u32,
    pub token_budget: u64,
    /// Every ledger write this session makes carries this tag (phase 5).
    pub provenance: Provenance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Provenance {
    pub app_id: String,
    pub app_version: String,
}

impl IsolateSettings {
    /// The isolate settings for an app's contract policy: exactly what the
    /// policy grants, in the isolate's knobs.
    pub fn for_app(policy: &octosense_app_contract::AppPolicy, app_data_root: &Path) -> IsolateSettings {
        IsolateSettings {
            capabilities: policy.capabilities.iter().cloned().collect(),
            allow_net: true,
            storage_quota: policy.storage_bytes,
            host_prompts: true,
            instruction_budget: policy.instruction_budget,
            memory_bytes: policy.memory_bytes,
            jail_root: app_data_root.join(&policy.app_id),
            hosts: policy.hosts.iter().cloned().collect(),
        }
    }
}

impl AppPolicy {
    /// The app's jail: one directory per app under the host's data root.
    pub fn jail_root(&self, app_data_root: &Path) -> PathBuf {
        app_data_root.join(&self.app_id)
    }

    /// The settings for this app's isolate.
    pub fn isolate_settings(&self, app_data_root: &Path) -> IsolateSettings {
        IsolateSettings::for_app(&self.app, app_data_root)
    }

    /// The agent session for this app, when it asked for one. The workspace
    /// is the app's jail, so the agent's reach is the app's reach.
    pub fn session_profile(&self, app_data_root: &Path) -> Option<SessionProfile> {
        let agent = self.agent.as_ref()?;
        Some(SessionProfile {
            session_id: format!("{}.agent", self.app_id),
            mode: agent.profile.as_kernel_mode(),
            workspace: self.jail_root(app_data_root),
            read_allow_paths: Vec::new(),
            tools: agent.tools.iter().cloned().collect(),
            hosts: self.hosts.iter().cloned().collect(),
            max_iterations: agent.max_iterations,
            token_budget: agent.token_budget,
            provenance: Provenance { app_id: self.app_id.clone(), app_version: self.version.clone() },
        })
    }
}
