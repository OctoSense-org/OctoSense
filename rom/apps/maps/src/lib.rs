//! OctosMap for OctoSense: a full-screen map under a search bar, a place
//! sheet, directions by car, on foot and by bike, and turn-by-turn
//! navigation. The same crate is a standalone window (`src/main.rs`), a
//! process-hosted tile, and an in-process `AppModule` (`module.rs`) that
//! phones link automatically.
pub use makepad_widgets;

pub mod geo;
pub mod guidance;
pub mod model;
pub mod module;
pub mod places;
pub mod routing;
pub mod sheet;
pub mod view;

/// The hosted vector archive the map reads over HTTP range requests: the
/// one the framework's route app falls back to when it has no local map.
pub const HOSTED_TILES: &str = "https://makepad.nl/maps/world-20260903.mkmap";
