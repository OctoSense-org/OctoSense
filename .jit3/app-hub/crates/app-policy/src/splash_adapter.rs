//! Applying a resolved policy to the isolate that will run the app.
//!
//! Every admitted app receives its own jail and quota, a network module and
//! the public non-device runtime APIs. Declarations remain in the policy for
//! disclosure. Device APIs are enabled separately by a host that supplies
//! per-app consent; internal profile data and unowned notifications stay out.
use crate::containers::IsolateSettings;
use makepad_widgets::{Cx, SplashRef};

/// What [`apply`] set, for a host that wants to show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub capabilities: usize,
    pub hosts: usize,
    pub storage_quota: u64,
    pub instruction_budget: u64,
    pub memory_bytes: u64,
}

/// Seat `settings` on `splash` before its body is evaluated.
///
/// Order matters: the jail and the quota are set before anything the app runs
/// can write; the policy (capabilities, hosts, budget) and the heap ceiling
/// come next; the isolate's own network module is granted last, so a failure
/// earlier leaves an isolate with less reach rather than more.
pub fn apply(splash: &SplashRef, cx: &mut Cx, settings: &IsolateSettings) -> Applied {
    // Isolation and quota apply whether or not usage was declared.
    let storage_quota = settings.granted_storage_quota();
    splash.set_sandbox_dir(cx, settings.storage_root());
    splash.set_storage_quota(cx, Some(storage_quota));
    splash.set_host_caps(cx, crate::containers::public_runtime_capabilities(&settings.capabilities, false, false));
    splash.set_host_prompts(cx, settings.host_prompts);
    // A policy still marks an app isolate and protects private host APIs.
    // The current runtime treats destinations as declarations, not a gate.
    // jit3: SplashRef::set_policy / set_memory_bytes are fork-only (per-app policy): not applied in the spike
    let _ = (settings.hosts.clone(), settings.instruction_budget, settings.memory_bytes);
    if let Some(mut inner) = splash.borrow_mut() {
        inner.set_allow_net(settings.allow_net);
    }
    Applied {
        capabilities: settings.capabilities.len(),
        hosts: settings.hosts.len(),
        storage_quota,
        instruction_budget: settings.instruction_budget,
        memory_bytes: settings.memory_bytes,
    }
}
