//! The creation-time handoff of a scoped service to a native module.
//!
//! A shell creates a module instance with a fixed signature
//! (`AppModule::create(vm, open, handles)`). Right before that call it
//! [`offer`]s the instance's service under the module id and the instance
//! scope; inside `create` the module [`claim`]s it; right after, the shell
//! [`withdraw`]s whatever was not claimed. The offer is keyed by the exact
//! instance, lives only for the duration of one `create`, and is made only
//! by the shell, so a module is hosted exactly when the shell created it
//! with a service. Nothing is discovered: no environment, no published
//! connection, no connection attempt.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::contract::OctosAppService;

type Offers = Mutex<HashMap<(String, String), Arc<dyn OctosAppService>>>;

fn offers() -> &'static Offers {
    static OFFERS: OnceLock<Offers> = OnceLock::new();
    OFFERS.get_or_init(Default::default)
}

/// Offer `service` to the instance `scope` of `module` (the shell, just
/// before `create`). A second offer for the same instance replaces the first.
pub fn offer(module: &str, scope: &str, service: Arc<dyn OctosAppService>) {
    offers()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert((module.to_owned(), scope.to_owned()), service);
}

/// Take the service offered to this instance (the module, inside `create`).
/// `None`: the shell offered none — the app has no assistant here.
pub fn claim(module: &str, scope: &str) -> Option<Arc<dyn OctosAppService>> {
    offers()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&(module.to_owned(), scope.to_owned()))
}

/// Drop an unclaimed offer (the shell, right after `create`). Returns whether
/// one was left, i.e. the module did not take it.
pub fn withdraw(module: &str, scope: &str) -> bool {
    offers()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&(module.to_owned(), scope.to_owned()))
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::*;
    use std::collections::BTreeSet;

    struct Dummy;
    impl OctosAppService for Dummy {
        fn deployment(&self) -> Deployment {
            Deployment::Hosted
        }
        fn availability(&self) -> Availability {
            Availability::Idle
        }
        fn services(&self) -> BTreeSet<String> {
            BTreeSet::new()
        }
        fn model(&self) -> Option<ModelInfo> {
            None
        }
        fn settings_entry(&self) -> SettingsEntry {
            SettingsEntry::Host
        }
        fn set_account(&self, _: Option<&str>) {}
        fn open_context(&self, _: ContextSpec) -> Result<Arc<dyn OctosContext>, String> {
            Err("none".into())
        }
        fn release(&self) {}
        fn shutdown(&self) {}
    }

    #[test]
    fn an_offer_reaches_only_its_instance_once() {
        offer("rinx", "i1g1", Arc::new(Dummy));
        assert!(
            claim("rinx", "i1g2").is_none(),
            "another instance gets nothing"
        );
        assert!(
            claim("other", "i1g1").is_none(),
            "another module gets nothing"
        );
        assert!(claim("rinx", "i1g1").is_some());
        assert!(claim("rinx", "i1g1").is_none(), "claimed once");
        assert!(!withdraw("rinx", "i1g1"));
    }

    #[test]
    fn an_unclaimed_offer_is_withdrawn() {
        offer("rinx", "i9g9", Arc::new(Dummy));
        assert!(withdraw("rinx", "i9g9"));
        assert!(claim("rinx", "i9g9").is_none());
    }
}
