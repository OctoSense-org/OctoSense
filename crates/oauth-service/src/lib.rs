//! OAuth protocol and connection ownership for OctoSense host services.
//!
//! App-facing operations expose `Connection`, never `Tokens`. Browser entry,
//! credential storage, provider registration and consent belong to the host.
pub mod api;
#[cfg(feature = "acceptance-fixtures")]
pub mod acceptance_fixtures;
#[cfg(feature = "acceptance-fixtures")]
pub mod acceptance_inbox;
#[cfg(feature = "acceptance-fixtures")]
pub mod acceptance_calendar;
#[cfg(feature = "acceptance-fixtures")]
pub mod acceptance_github;
#[cfg(test)]
mod api_tests;
pub mod authorize;
pub mod backend;
pub mod calendar_cache;
#[cfg(feature = "host")]
mod backend_registry;
#[cfg(feature = "host")]
mod host_catalog;
#[cfg(feature = "host")]
pub mod host;
#[cfg(feature = "host")]
pub mod approval;
#[cfg(feature = "host")]
pub mod host_api;
#[cfg(feature = "host")]
pub mod host_inbox;
#[cfg(all(test, feature = "host"))]
mod host_vault_acceptance;
pub mod inbox;
pub mod inbox_events;
pub mod oauth;
pub mod providers;
mod protocol;
#[cfg(any(feature = "host", test))]
mod registration;
pub mod store;
pub mod transport;

pub use providers::Provider;
pub use store::{Connection, Connections, CredentialStore};
