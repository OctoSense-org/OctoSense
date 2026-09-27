//! The thinking octopus: the OctoSense logo (`resources/icons/octopus.svg`,
//! the silhouette the web `OctopusLogo` draws in currentColor), shown by
//! the live island in place of the AppCard glyph while the hosted
//! AppCard's kernel is on a turn. It breathes and bobs — a filled
//! silhouette is animated by its transform, not redrawn — and its phase is
//! the drawing frame's clock, so an idle screen stays idle; the island
//! keeps the frame loop running for it while a turn is in flight.
use crate::shell::ui::{rect, Ico, ShellDraw};
use makepad_widgets::*;

/// Draw the octopus centred in `slot` at animation time `now` (seconds),
/// in `color` (its alpha included): a slow breath of ±5 % and a bob of a
/// point or two, a little larger than the slot so it reads at glyph size.
pub fn draw(cx: &mut Cx2d, d: &mut ShellDraw, slot: Rect, now: f64, color: Vec4f) {
    let t = now % 8.0;
    let breath = 1.0 + 0.05 * (t * 2.4).sin();
    let bob = 1.2 * (t * 1.7).sin();
    let sway = 0.6 * (t * 1.1 + 0.8).sin();
    let size = slot.size.x.min(slot.size.y) * 1.15 * breath;
    let centre = rect(slot.pos.x + sway, slot.pos.y + bob, slot.size.x, slot.size.y);
    d.icon_centered(cx, Ico::Octopus, centre, size, color);
}
