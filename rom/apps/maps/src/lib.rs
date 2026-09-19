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
#[cfg(test)]
pub(crate) mod test_support;
pub mod view;

pub use module::{MapsModule, MAPS_MODULE};
pub use view::MapsView;

/// The hosted vector archive the map reads over HTTP range requests: the
/// one the framework's route app falls back to when it has no local map.
pub const HOSTED_TILES: &str = "https://makepad.nl/maps/world-20260903.mkmap";

/// The sea, which the base archive leaves to two overlays of its own: a
/// coarse one for the far zooms and a fine one for the near, both drawn as
/// the map's `ocean` layer, as the route app hosts them.
pub const HOSTED_OCEAN: [&str; 2] = [
    "https://makepad.nl/maps/overlays/ocean-low-20260903.mkmap/",
    "https://makepad.nl/maps/overlays/ocean-high-20260904.mkmap/",
];
