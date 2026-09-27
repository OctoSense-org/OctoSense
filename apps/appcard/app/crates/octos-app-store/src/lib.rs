//! octos-app-store
//!
//! AppState reducer + selectors. No Makepad imports — keeps tests trivial and lets
//! W01 / W04 / W05 implementers iterate without a UI loop.
//!
//! See `~/home/octos-app/01-ARCHITECTURE.md` § "State model".

pub mod auth;

#[cfg(feature = "keychain")]
pub mod keychain;

pub mod approvals;
pub mod files;
pub mod navigation;
pub mod sessions;
pub mod state;
pub mod tasks;
pub mod toasts;
pub mod turns;

pub use state::{
    AppState, ConnectionEvent, ConnectionState, Ephemeral, Event, SnapshotEvent, UiCursorMap,
    reduce,
};
