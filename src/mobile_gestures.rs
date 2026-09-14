//! The phone shell's gesture contract. Every mobile surface (shade, pages,
//! island, tile groups, switcher) is reached through one of these gestures,
//! so the recognizer is the single owner of edge and bottom touches and every
//! surface consumes the same enum. Exclusion zones let an app own an edge
//! (a map's pan, a slider's track) so shell gestures never fight it.
//!
//! Contract for the feature modules (`mobile_shade`, `mobile_pages`,
//! `mobile_island`, `mobile_groups`): read `ShellGesture` values from
//! `PhoneState::gesture_out`, never touch raw fingers.
use makepad_widgets::*;

/// Which screen edge a gesture started from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge { Top, Bottom, Left, Right }

/// Which half of the top edge the shade was pulled from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadeSide { Notifications, Controls }

/// Horizontal direction, in screen space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir { Left, Right }

/// A shell gesture, delivered every frame while it is in progress and once
/// more as `Commit` or `Cancel`. `progress` is 0..1 of the distance that
/// commits the gesture; surfaces animate from it directly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShellGesture {
    /// Bottom edge, upward: go home. Held still near the end: the switcher.
    HomeUp { progress: f64, held: bool },
    /// A flick along the bottom edge: the previous or next app.
    QuickSwitch { dir: Dir, progress: f64 },
    /// Inward from a side edge: back, with a predictive preview.
    Back { edge: Edge, progress: f64 },
    /// Downward from the top edge: the shade, notifications or controls.
    ShadePull { side: ShadeSide, progress: f64 },
    /// Horizontal on the home page: previous or next page (the glance page
    /// is the page left of the first).
    PageSwipe { dir: Dir, progress: f64 },
    /// Downward in the middle of the home page: search.
    HomeSearch { progress: f64 },
    /// The gesture reached its commit distance and the finger lifted.
    Commit(GestureKind),
    /// The finger lifted short of the commit distance, or another finger
    /// took over: surfaces animate back.
    Cancel(GestureKind),
}

/// The gesture family, for `Commit` and `Cancel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureKind { HomeUp, Switcher, QuickSwitch(Dir), Back, Shade(ShadeSide), Page(Dir), HomeSearch }

/// A rect an app owns: shell gestures that start inside it are not
/// recognised, so a map can pan from the edge and a slider can be dragged.
#[derive(Clone, Debug, PartialEq)]
pub struct ExclusionZone { pub rect: Rect, pub edges: [bool; 4] }

/// Exclusion zones per client, in screen space, refreshed each frame by the
/// surface that draws the client.
#[derive(Clone, Debug, Default)]
pub struct ExclusionZones { pub zones: Vec<ExclusionZone> }

impl ExclusionZones {
    pub fn clear(&mut self) { self.zones.clear(); }
    pub fn add(&mut self, rect: Rect, edges: [bool; 4]) { self.zones.push(ExclusionZone { rect, edges }); }
    /// True when a gesture from `edge` starting at `p` must be left to the app.
    pub fn excludes(&self, p: Vec2d, edge: Edge) -> bool {
        let i = match edge { Edge::Top => 0, Edge::Bottom => 1, Edge::Left => 2, Edge::Right => 3 };
        self.zones.iter().any(|z| z.edges[i] && z.rect.contains(p))
    }
}

/// Edge band widths and commit distances, in points; one place to tune.
#[derive(Clone, Copy, Debug)]
pub struct GestureMetrics {
    pub edge_band: f64,
    pub bottom_band: f64,
    pub top_band: f64,
    pub commit_distance: f64,
    pub hold_time: f64,
    pub flick_velocity: f64,
}
impl Default for GestureMetrics {
    fn default() -> Self { Self { edge_band: 24.0, bottom_band: 28.0, top_band: 32.0, commit_distance: 120.0, hold_time: 0.28, flick_velocity: 900.0 } }
}
