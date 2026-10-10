//! `octosense:host`: a component calls its app's host services, as the
//! app's script does with `host.request` (`wit/octosense-host.wit`).
//!
//! The runtime knows no services: the embedder hands an instance its app's
//! [`HostCalls`], which decides what the app may call and makes the call.
//! Without one, every call answers that this host has none. A call waits
//! outside the guest, where the deadline's epoch check cannot end it, so
//! [`HostCalls::request`] gets the call's deadline and must answer by then.

use std::sync::Arc;
use std::time::Instant;

use wasmtime::component::Linker;
use wasmtime::StoreContextMut;

use super::State;

/// The interface a component imports (older 0.1 patch versions resolve to
/// it).
pub(crate) const INTERFACE: &str = "octosense:host/services@0.1.0";

/// An app's host services, as a component reaches them.
pub trait HostCalls: Send + Sync {
    /// Calls `service` (`family.method`) with JSON `args`: its JSON answer,
    /// or why there is none. Answers by `deadline`.
    fn request(&self, service: &str, args: &str, deadline: Instant) -> Result<String, String>;
}

/// Links `octosense:host/services`.
pub(crate) fn link(linker: &mut Linker<State>) -> wasmtime::Result<()> {
    linker.instance(INTERFACE)?.func_wrap(
        "request",
        |store: StoreContextMut<'_, State>,
         (service, args): (String, String)|
         -> wasmtime::Result<(Result<String, String>,)> {
            let state = store.data();
            let deadline = state
                .guard
                .as_ref()
                .map(|guard| guard.deadline)
                .unwrap_or_else(Instant::now);
            let answer = match &state.host_calls {
                Some(host) => host.request(&service, &args, deadline),
                None => Err("this host gives a component no host services".into()),
            };
            Ok((answer,))
        },
    )?;
    Ok(())
}

/// The calls an instance's host services get, set by the embedder.
pub(crate) type Calls = Option<Arc<dyn HostCalls>>;
