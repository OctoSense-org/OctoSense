//! A crate with a private module named `serde_json` whose items it re-exports (as makepad_micro_serde does).
mod serde_json { pub struct SerJsonMarker; }
pub use crate::serde_json::*;
