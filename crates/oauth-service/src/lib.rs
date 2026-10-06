//! OAuth protocol and connection ownership for OctoSense host services.
//!
//! App-facing operations expose `Connection`, never `Tokens`. Browser entry,
//! credential storage, provider registration and consent belong to the host.
pub mod api;
#[cfg(test)]
mod api_tests;
pub mod authorize;
pub mod calendar_cache;
#[cfg(feature = "host")]
pub mod host;
#[cfg(feature = "host")]
pub mod host_api;
#[cfg(feature = "host")]
pub mod host_inbox;
pub mod inbox;
pub mod inbox_events;
pub mod oauth;
pub mod providers;
pub mod store;
pub mod transport;

pub use providers::Provider;
pub use store::{Connection, Connections, CredentialStore};
