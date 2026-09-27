//! The shell side: an app's scoped assistant service from its declaration.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use crate::broker::{Broker, BrokerConfig};
use crate::connectors::CoreConnector;
use crate::contract::{octos_services_in, Deployment, OctosAppService};

/// The kernel profile a shell's apps share (the AI providers app writes it).
pub const SHARED_PROFILE: &str = "_main";

/// The shell's system agent session: the owner (originator) of every app
/// peer.
pub fn system_session() -> String {
    format!("{SHARED_PROFILE}:api:octosense#system")
}

/// Which apps the shell lets use the assistant, and with which services.
/// Host policy and the person's grants; an app not listed gets nothing.
#[derive(Default)]
pub struct HostPolicy {
    grants: Mutex<HashMap<String, BTreeSet<String>>>,
}

impl HostPolicy {
    /// Allow `module` the listed services (exact names; others ignored).
    pub fn allow<'a>(&self, module: &str, services: impl IntoIterator<Item = &'a str>) {
        self.grants
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(module.to_owned(), octos_services_in(services));
    }

    /// Withdraw every grant of `module`.
    pub fn deny(&self, module: &str) {
        self.grants
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(module);
    }

    /// The grant of `module`.
    pub fn granted(&self, module: &str) -> BTreeSet<String> {
        self.grants
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(module)
            .cloned()
            .unwrap_or_default()
    }
}

/// The effective assistant services of a module: declared ∩ supported ∩
/// granted. Empty means no assistant (and no peer) for it.
pub fn effective_services<'a>(
    module: &str,
    declared: impl IntoIterator<Item = &'a str>,
    policy: &HostPolicy,
) -> BTreeSet<String> {
    let declared = octos_services_in(declared);
    let granted = policy.granted(module);
    declared.intersection(&granted).cloned().collect()
}

/// The scoped service for one instance of `module`, or `None` when it has
/// no effective assistant services (then no peer is ever allocated). The
/// service uses the shell's kernel; the kernel starts on the first request
/// and is shared with every other app.
pub fn launch<'a>(
    module: &str,
    label: &str,
    declared: impl IntoIterator<Item = &'a str>,
    policy: &HostPolicy,
) -> Option<Broker> {
    let services = effective_services(module, declared, policy);
    if services.is_empty() || !is_namespace_segment(module) {
        return None;
    }
    let mut cfg = BrokerConfig::new(
        Deployment::Hosted,
        SHARED_PROFILE,
        system_session(),
        module,
        label,
        services,
    );
    // The shell keeps each app peer's host token beside its kernel's core
    // dir, outside every app's reach.
    cfg.state_dir = octosense_kernel::core_dir().map(|dir| host_state_dir(&dir));
    Some(Broker::new(cfg, Arc::new(CoreConnector::shell())))
}

/// Where a shell keeps app peers' host tokens for the kernel at `core_dir`.
pub fn host_state_dir(core_dir: &std::path::Path) -> std::path::PathBuf {
    core_dir.parent().unwrap_or(core_dir).join("app-peers")
}

/// Offer `broker` to the instance being created, as a trait object.
pub fn offer(module: &str, scope: &str, broker: &Broker) {
    crate::injection::offer(
        module,
        scope,
        Arc::new(broker.clone()) as Arc<dyn OctosAppService>,
    );
}

fn is_namespace_segment(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_gets_only_declared_and_granted_services() {
        let policy = HostPolicy::default();
        policy.allow(
            "rinx",
            [
                "octos.session.open",
                "octos.session.history",
                "octos.turn.start",
            ],
        );
        let declared = [
            "net",
            "storage",
            "octos.session.open",
            "octos.session.history",
            "octos.turn.interrupt",
        ];
        assert_eq!(
            effective_services("rinx", declared, &policy)
                .into_iter()
                .collect::<Vec<_>>(),
            ["octos.session.history", "octos.session.open"]
        );
        assert!(
            effective_services("maps", ["octos.turn.start"], &policy).is_empty(),
            "not granted"
        );
    }

    #[test]
    fn apps_without_assistant_services_allocate_no_peer() {
        let policy = HostPolicy::default();
        policy.allow("rinx", crate::contract::OCTOS_SERVICES);
        assert!(launch("news", "News", ["net", "storage"], &policy).is_none());
        assert!(
            launch("rinx", "Rinx", ["net"], &policy).is_none(),
            "nothing declared"
        );
        let broker =
            launch("rinx", "Rinx", crate::contract::OCTOS_SERVICES, &policy).expect("granted");
        assert_eq!(broker.deployment(), Deployment::Hosted);
        assert_eq!(broker.config().originator, system_session());
    }
}
