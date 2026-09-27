//! Library zoom: the pinch/wheel scale, the grid density it selects, and the
//! multi-touch bookkeeping that tells a pinch apart from a scroll or a tap.

use makepad_widgets::makepad_platform::event::{TouchState, TouchUpdateEvent};
use makepad_widgets::*;

/// Photos per row at scale 1.0, matching the Library's established density.
const DEFAULT_COLUMNS: usize = 3;
const MIN_COLUMNS: usize = 1;
const MAX_COLUMNS: usize = 9;
const MIN_SCALE: f64 = DEFAULT_COLUMNS as f64 / MAX_COLUMNS as f64;
const MAX_SCALE: f64 = DEFAULT_COLUMNS as f64 / MIN_COLUMNS as f64;
/// How far past a density the scale must travel before the wall reflows. Half
/// a column would flicker whenever a hand rests on a boundary.
const HYSTERESIS: f64 = 0.6;
/// Exponential wheel response, taken from the reference photo wall so a notch
/// zooms by the same proportion at every density.
const WHEEL_RATE: f64 = 0.0025;

/// The Library's zoom level: a continuous scale the gestures move, plus the
/// whole number of columns the grid actually draws.
#[derive(Clone, Copy, Debug)]
pub struct LibraryZoom {
    scale: f64,
    columns: usize,
}

impl Default for LibraryZoom {
    fn default() -> Self {
        Self {
            scale: 1.0,
            columns: DEFAULT_COLUMNS,
        }
    }
}

impl LibraryZoom {
    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    /// Sets the zoom, ignoring a scale a gesture could not mean and holding the
    /// current density until the request is clearly past its boundary.
    pub fn set_scale(&mut self, scale: f64) {
        if scale.is_nan() {
            return;
        }
        self.scale = scale.clamp(MIN_SCALE, MAX_SCALE);
        let density =
            (DEFAULT_COLUMNS as f64 / self.scale).clamp(MIN_COLUMNS as f64, MAX_COLUMNS as f64);
        if (density - self.columns as f64).abs() > HYSTERESIS {
            self.columns = density.round() as usize;
        }
    }

    /// The zoom as a slider position: 0.0 is the densest wall, 1.0 one photo
    /// per row. Logarithmic, so equal travel is an equal proportion of zoom.
    pub fn normalized(&self) -> f64 {
        ((self.scale / MIN_SCALE).ln() / (MAX_SCALE / MIN_SCALE).ln()).clamp(0.0, 1.0)
    }

    pub fn set_normalized(&mut self, position: f64) {
        self.set_scale(MIN_SCALE * (MAX_SCALE / MIN_SCALE).powf(position.clamp(0.0, 1.0)));
    }

    /// Wheel or trackpad zoom: scrolling down shrinks the photos.
    pub fn wheel(&mut self, delta_y: f64) {
        self.set_scale(self.scale * (-delta_y * WHEEL_RATE).exp());
    }
}

/// What a touch update meant for the zoom.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pinch {
    /// The Library owns this event; nothing below it should act on the touch.
    pub consumed: bool,
    /// A pinch started on this event.
    pub began: bool,
    /// The scale the gesture now asks for, while both fingers are down.
    pub scale: Option<f64>,
    /// The midpoint between the fingers, in absolute coordinates.
    pub center: DVec2,
}

#[derive(Clone, Copy, Debug)]
struct Contact {
    uid: u64,
    pos: DVec2,
}

#[derive(Clone, Copy, Debug)]
struct Gesture {
    first: u64,
    second: u64,
    distance: f64,
    base: f64,
}

/// Follows the fingers on the Library grid and reports pinches.
#[derive(Clone, Debug, Default)]
pub struct PinchTracker {
    /// Touches that started inside the grid, in the order they arrived.
    contacts: Vec<Contact>,
    gesture: Option<Gesture>,
    /// A pinch keeps the touch stream to itself until the last finger lifts, so
    /// releasing it neither opens a photo nor flings the list.
    holding: bool,
}

impl PinchTracker {
    /// Folds one touch update into the gesture state. `bounds` is the grid the
    /// pinch must start inside; `scale` is the zoom a starting pinch builds on.
    pub fn update(&mut self, event: &TouchUpdateEvent, bounds: Rect, scale: f64) -> Pinch {
        for touch in &event.touches {
            match touch.state {
                TouchState::Start => {
                    if bounds.contains(touch.abs) && self.contact(touch.uid).is_none() {
                        self.contacts.push(Contact {
                            uid: touch.uid,
                            pos: touch.abs,
                        });
                    }
                }
                TouchState::Stop => self.contacts.retain(|c| c.uid != touch.uid),
                TouchState::Move | TouchState::Stable => {
                    if let Some(contact) = self.contacts.iter_mut().find(|c| c.uid == touch.uid) {
                        contact.pos = touch.abs;
                    }
                }
            }
        }

        let mut result = Pinch {
            consumed: self.holding,
            ..Pinch::default()
        };
        match self.gesture {
            None if self.contacts.len() >= 2 => {
                let (first, second) = (self.contacts[0], self.contacts[1]);
                self.gesture = Some(Gesture {
                    first: first.uid,
                    second: second.uid,
                    // A zero spread would make every later ratio infinite.
                    distance: first.pos.distance(&second.pos).max(1.0),
                    base: scale,
                });
                self.holding = true;
                result.began = true;
                result.consumed = true;
                result.center = (first.pos + second.pos) * 0.5;
            }
            Some(gesture) => match (self.contact(gesture.first), self.contact(gesture.second)) {
                (Some(first), Some(second)) => {
                    result.center = (first.pos + second.pos) * 0.5;
                    result.scale =
                        Some(gesture.base * first.pos.distance(&second.pos) / gesture.distance);
                }
                // A finger left: the pinch is over, but its release is not a tap.
                _ => self.gesture = None,
            },
            None => {}
        }
        if self.contacts.is_empty() {
            self.gesture = None;
            self.holding = false;
        }
        result
    }

    fn contact(&self, uid: u64) -> Option<Contact> {
        self.contacts.iter().find(|c| c.uid == uid).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_widgets::makepad_platform::event::TouchPoint;
    use std::cell::Cell;

    fn event(points: &[(u64, TouchState, f64, f64)]) -> TouchUpdateEvent {
        TouchUpdateEvent {
            time: 1.0,
            window_id: WindowId(0, 0),
            modifiers: KeyModifiers::default(),
            touches: points
                .iter()
                .map(|&(uid, state, x, y)| TouchPoint {
                    uid,
                    state,
                    abs: dvec2(x, y),
                    time: 1.0,
                    rotation_angle: 0.0,
                    force: 1.0,
                    radius: dvec2(1.0, 1.0),
                    handled: Cell::new(Area::Empty),
                    sweep_lock: Cell::new(Area::Empty),
                })
                .collect(),
        }
    }
    fn bounds() -> Rect {
        Rect {
            pos: dvec2(0.0, 0.0),
            size: dvec2(400.0, 600.0),
        }
    }

    #[test]
    fn wheel_and_pinch_scale_change_density_with_finite_bounds() {
        let mut zoom = LibraryZoom::default();
        assert_eq!(zoom.columns(), 3);
        zoom.set_scale(1.5);
        assert_eq!(zoom.columns(), 2);
        zoom.set_scale(0.5);
        assert_eq!(zoom.columns(), 6);
        zoom.set_scale(1000.0);
        assert_eq!(zoom.columns(), 1);
        zoom.wheel(100_000.0);
        assert_eq!(zoom.columns(), 9);
        zoom.set_scale(f64::NAN);
        assert!(zoom.scale().is_finite());
        zoom.wheel(-100_000.0);
        assert_eq!(zoom.columns(), 1);
    }

    #[test]
    fn the_slider_position_spans_the_whole_density_range() {
        let mut zoom = LibraryZoom::default();
        // The default density sits at the middle of the slider's travel.
        assert!((zoom.normalized() - 0.5).abs() < 0.001);
        zoom.set_normalized(0.0);
        assert_eq!(zoom.columns(), 9);
        zoom.set_normalized(1.0);
        assert_eq!(zoom.columns(), 1);
        // A wheel gesture moves the same position, so the knob follows it.
        zoom.wheel(100_000.0);
        assert!(zoom.normalized() < 0.001);
    }

    #[test]
    fn small_changes_near_a_density_boundary_do_not_flicker() {
        let mut zoom = LibraryZoom::default();
        zoom.set_scale(3.0 / 3.55);
        assert_eq!(zoom.columns(), 3);
        zoom.set_scale(3.0 / 3.7);
        assert_eq!(zoom.columns(), 4);
        zoom.set_scale(3.0 / 3.5);
        assert_eq!(zoom.columns(), 4);
    }

    #[test]
    fn pinch_uses_two_contacts_and_consumes_until_the_last_finger_lifts() {
        let mut pinch = PinchTracker::default();
        let a = pinch.update(
            &event(&[(1, TouchState::Start, 100.0, 200.0)]),
            bounds(),
            1.0,
        );
        assert!(!a.consumed);
        let b = pinch.update(
            &event(&[
                (1, TouchState::Stable, 100.0, 200.0),
                (2, TouchState::Start, 200.0, 200.0),
            ]),
            bounds(),
            1.0,
        );
        assert!(b.began && b.consumed);
        let c = pinch.update(
            &event(&[
                (1, TouchState::Move, 50.0, 200.0),
                (2, TouchState::Move, 250.0, 200.0),
            ]),
            bounds(),
            1.0,
        );
        assert_eq!(c.scale, Some(2.0));
        assert_eq!(c.center, dvec2(150.0, 200.0));
        assert!(
            pinch
                .update(&event(&[(1, TouchState::Stop, 50.0, 200.0)]), bounds(), 2.0)
                .consumed
        );
        assert!(
            pinch
                .update(
                    &event(&[(2, TouchState::Move, 250.0, 210.0)]),
                    bounds(),
                    2.0
                )
                .consumed
        );
        assert!(
            pinch
                .update(
                    &event(&[(2, TouchState::Stop, 250.0, 210.0)]),
                    bounds(),
                    2.0
                )
                .consumed
        );
        assert!(
            !pinch
                .update(
                    &event(&[(3, TouchState::Start, 100.0, 200.0)]),
                    bounds(),
                    2.0
                )
                .consumed
        );
    }

    #[test]
    fn touches_started_outside_the_grid_do_not_start_a_pinch() {
        let mut pinch = PinchTracker::default();
        pinch.update(
            &event(&[(1, TouchState::Start, 100.0, -20.0)]),
            bounds(),
            1.0,
        );
        let result = pinch.update(
            &event(&[
                (1, TouchState::Move, 100.0, 200.0),
                (2, TouchState::Start, 200.0, 200.0),
            ]),
            bounds(),
            1.0,
        );
        assert!(!result.consumed);
    }
}
