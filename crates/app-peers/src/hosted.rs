//! The shell side: a scoped assistant service for an opted-in, allowed app.

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

/// Which opted-in apps the shell lets use the assistant. Nonempty supported
/// names establish that offer; an app not listed gets nothing. Methods come
/// from the fixed public surface, not a subset of declaration metadata.
#[derive(Default)]
pub struct HostPolicy {
    grants: Mutex<HashMap<String, BTreeSet<String>>>,
}

impl HostPolicy {
    /// Allow an opted-in `module` to use the public assistant surface.
    /// At least one supported method is required; unknown names are ignored.
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

/// An opted-in native module with the host's consent gets the fixed public
/// assistant surface. Declarations describe usage; they do not divide that
/// surface into permissions. No declaration or no host grant means no peer.
pub fn effective_services<'a>(
    module: &str,
    declared: impl IntoIterator<Item = &'a str>,
    policy: &HostPolicy,
) -> BTreeSet<String> {
    let declared = octos_services_in(declared);
    let granted = policy.granted(module);
    if declared.is_empty() || granted.is_empty() { return BTreeSet::new(); }
    octos_services_in(crate::contract::OCTOS_SERVICES)
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

/// The newest peer's token under `dir`: a peer record (`*.peer`,
/// [`crate::peer_record`]) or an older `*.token` file.
pub fn newest_token(dir: &std::path::Path) -> Option<String> {
    let mut best: Option<(std::time::SystemTime, String)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let token = match path.extension().and_then(|e| e.to_str()) {
            Some("token") => text.trim().to_owned(),
            Some("peer") => serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v["token"].as_str().map(|t| t.trim().to_owned()))
                .unwrap_or_default(),
            _ => continue,
        };
        if token.is_empty() {
            continue;
        }
        let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(t, _)| modified > *t) {
            best = Some((modified, token));
        }
    }
    best.map(|(_, token)| token)
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
    fn a_consented_opted_in_module_gets_the_supported_public_surface() {
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
            ["octos.session.history", "octos.session.open", "octos.turn.interrupt", "octos.turn.start"]
        );
        assert!(
            effective_services("maps", ["octos.turn.start"], &policy).is_empty(),
            "not granted"
        );
    }

    #[test]
    fn the_newest_kept_token_is_the_system_sessions_credential() {
        let dir = std::env::temp_dir().join(format!("app-peers-tokens-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(newest_token(&dir), None);
        std::fs::write(dir.join("app_rinx_acct-1.token"), "old\n").unwrap();
        std::fs::write(dir.join("app_rinx_acct-1.cwd"), "/not/a/token").unwrap();
        assert_eq!(newest_token(&dir).as_deref(), Some("old"));
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("app_card.news_acct-2.token"), "new").unwrap();
        assert_eq!(newest_token(&dir).as_deref(), Some("new"));
        // A peer record (token and workspace in one file) counts the same.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("app_notes_acct-3.peer"), r#"{"token":"newest","cwd":"/w"}"#).unwrap();
        assert_eq!(newest_token(&dir).as_deref(), Some("newest"));
        let _ = std::fs::remove_dir_all(&dir);
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
