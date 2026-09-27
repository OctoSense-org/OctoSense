//! octosense-app-peers: host-owned octos app peers (Rinx ADR 0007).
//!
//! An OctoSense shell runs one octos kernel and one provider profile. When it
//! launches a native app whose declared capabilities include assistant
//! services (the exact `octos.*` names App Hub publishes) and host policy
//! grants them, it creates or resumes ONE octos peer for the app, owned by
//! the shell's system agent session, and injects a scoped
//! [`OctosAppService`] into the module at creation ([`injection`]). The app
//! opens request contexts of that peer, one per client instance (a Rinx mini
//! app), and never sees raw kernel protocol, provider credentials or a kernel
//! of its own. Apps without granted assistant services allocate no peer.
//!
//! A standalone app that owns a local runtime, or talks to an explicitly
//! configured remote server, uses the same [`broker::Broker`] with its own
//! root session as the owner: one contract, three deployments
//! ([`Deployment`]).
//!
//! The kernel side of the contract is octos UPCR-2026-034 (`peer/prepare`
//! host binding with an app/account memory namespace and resume,
//! `peer/context/open|close`, `peer/model/set`).

pub mod contract;
pub mod injection;

#[cfg(feature = "broker")]
pub mod broker;
#[cfg(feature = "broker")]
pub mod connectors;
#[cfg(feature = "octos-core")]
pub mod hosted;

pub use contract::*;

/// The kernel service, for a standalone app that owns its local runtime.
#[cfg(feature = "octos-core")]
pub use octosense_octos_core as octos_core;
