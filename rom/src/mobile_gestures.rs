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

// ---------------------------------------------------------------------------
// The recognizer
// ---------------------------------------------------------------------------

use crate::mobile::PhoneScreen;
use std::collections::VecDeque;

/// One finger event in screen points. The shell owns at most one finger at
/// a time (`PhoneState::touch`), so the recognizer tracks a single finger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FingerPhase { Down, Move, Up }

/// The platform's safe-area insets around the phone viewport (the Android
/// status bar and navigation bar). A finger inside one of the bars still
/// counts as the band next to it, so a swipe up from the very bottom of
/// the glass goes home like the system's own gesture would.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SafeInsets { pub top: f64, pub right: f64, pub bottom: f64, pub left: f64 }
impl SafeInsets {
    /// `r` with the insets taken off each edge (never below a point).
    pub fn inset(&self, r: Rect) -> Rect {
        Rect {
            pos: r.pos + dvec2(self.left, self.top),
            size: dvec2((r.size.x - self.left - self.right).max(1.0), (r.size.y - self.top - self.bottom).max(1.0)),
        }
    }
}

/// What the recognizer knows about the screen when a finger arrives: the
/// phone viewport, the insets around it and which shell screen is showing
/// (the home page recognises page swipes and search; the others do not).
#[derive(Clone, Copy, Debug)]
/// `body`: the middle of the screen is the shell's to recognise gestures
/// in — the home page, and the App Library while it is not scrolling search
/// results; over an app it belongs to the app.
pub struct GestureContext { pub screen: Rect, pub insets: SafeInsets, pub phone: PhoneScreen, pub body: bool }

/// Where the finger touched down: the band decides the family of gesture
/// it can become. `Body` is the middle of the home page (a pull opens
/// search, a horizontal drag turns a page); `Column` its left or right
/// quarter, where a pull is the shade's side — notifications on the left,
/// controls on the right — without reaching for the top edge. The App
/// Library's body is its own: drags there scroll the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin { Bottom, Top(ShadeSide), Side(Edge), Body, Column(ShadeSide) }

#[derive(Clone, Debug)]
struct Track {
    origin: Origin,
    start: Vec2d,
    last: Vec2d,
    /// The newest positions with their times, for the lift velocity.
    samples: VecDeque<(Vec2d, f64)>,
    /// Locked once the finger has moved past the slop in a direction the
    /// origin allows; never reclassified while the finger is down.
    kind: Option<GestureKind>,
    held: bool,
    /// The finger has not strayed more than `STILL` from here since
    /// `still_since`: the pause that turns a home swipe into the switcher.
    still_anchor: Vec2d,
    still_since: f64,
    /// The first `tick` after the finger last moved, on the frame clock.
    /// Finger and frame clocks need not agree (Android stamps touches
    /// with the event uptime), so the pause is measured on the clock that
    /// reports it.
    still_tick: Option<f64>,
    live: Option<ShellGesture>,
}

/// Movement before a touch is a drag, not a tap.
const SLOP: f64 = 10.0;
/// Jitter tolerated while "holding still".
const STILL: f64 = 4.0;
/// How far back the lift velocity looks.
const VELOCITY_WINDOW: f64 = 0.12;
const SAMPLES: usize = 8;
/// A home swipe this far along counts as "near commit" for the hold.
const HOLD_NEAR: f64 = 0.6;

/// Turns the shell's finger into `ShellGesture`s. Feed it every Down/Move/Up
/// of the finger the shell owns; it answers with the gesture in progress
/// (each Move), then `Commit` or `Cancel` (the Up). `tick` lets a finger
/// that stopped moving become the switcher without a new event.
#[derive(Clone, Debug, Default)]
pub struct GestureRecognizer {
    pub metrics: GestureMetrics,
    track: Option<Track>,
}

impl GestureRecognizer {
    pub fn new(metrics: GestureMetrics) -> Self { Self { metrics, track: None } }
    /// The recognizer claimed the finger that is down.
    pub fn active(&self) -> bool { self.track.is_some() }
    /// The gesture in progress, as last emitted.
    pub fn current(&self) -> Option<ShellGesture> { self.track.as_ref().and_then(|t| t.live) }
    /// The finger that is down started in a shell band (bottom, top or a
    /// side) rather than in the home page body.
    pub fn from_band(&self) -> bool { self.track.as_ref().is_some_and(|t| t.origin != Origin::Body) }

    /// Feed one finger event. `Down` decides whether the shell claims the
    /// finger (`active()` afterwards); an excluded edge, a body touch off
    /// the home page or a touch outside the screen are left to the app.
    pub fn feed(&mut self, phase: FingerPhase, p: Vec2d, time: f64, ctx: &GestureContext, exclusions: &ExclusionZones) -> Option<ShellGesture> {
        match phase {
            FingerPhase::Down => {
                self.track = None;
                let origin = self.origin_at(p, ctx, exclusions)?;
                let mut samples = VecDeque::with_capacity(SAMPLES);
                samples.push_back((p, time));
                self.track = Some(Track { origin, start: p, last: p, samples, kind: None, held: false, still_anchor: p, still_since: time, still_tick: None, live: None });
                None
            }
            FingerPhase::Move => {
                let m = self.metrics;
                let t = self.track.as_mut()?;
                Self::sample(t, p, time);
                let delta = p - t.start;
                if t.kind.is_none() && delta.length() > SLOP { t.kind = Self::classify(t.origin, delta); }
                let kind = t.kind?;
                let progress = Self::progress(kind, t.origin, delta, m.commit_distance);
                Self::check_hold(t, kind, progress, time, m.hold_time);
                let live = Self::live(kind, t.origin, progress, t.held);
                t.live = Some(live);
                Some(live)
            }
            FingerPhase::Up => {
                let m = self.metrics;
                let mut t = self.track.take()?;
                Self::sample(&mut t, p, time);
                let kind = t.kind?;
                let delta = p - t.start;
                let progress = Self::progress(kind, t.origin, delta, m.commit_distance);
                let along = Self::along(kind, t.origin, Self::velocity(&t));
                if kind == GestureKind::HomeUp && t.held { return Some(ShellGesture::Commit(GestureKind::Switcher)); }
                if progress >= Self::commit_fraction(kind) || along >= m.flick_velocity { Some(ShellGesture::Commit(kind)) } else { Some(ShellGesture::Cancel(kind)) }
            }
        }
    }

    /// Time passed without a finger event: a home swipe that is resting
    /// near or past its commit distance becomes the switcher intent.
    pub fn tick(&mut self, time: f64) -> Option<ShellGesture> {
        let m = self.metrics;
        let t = self.track.as_mut()?;
        let kind = t.kind?;
        if t.held || kind != GestureKind::HomeUp { return None; }
        let since = *t.still_tick.get_or_insert(time);
        let delta = t.last - t.start;
        let progress = Self::progress(kind, t.origin, delta, m.commit_distance);
        if progress < HOLD_NEAR || time - since < m.hold_time { return None; }
        t.held = true;
        let live = Self::live(kind, t.origin, progress, t.held);
        t.live = Some(live);
        Some(live)
    }

    /// Another finger took over, the screen rotated, or the shell changed
    /// under the finger: drop the track, telling the surfaces to animate back.
    pub fn cancel(&mut self) -> Option<ShellGesture> {
        let t = self.track.take()?;
        t.kind.map(ShellGesture::Cancel)
    }

    fn origin_at(&self, p: Vec2d, ctx: &GestureContext, exclusions: &ExclusionZones) -> Option<Origin> {
        let m = self.metrics;
        let s = ctx.screen;
        let (left, top) = (s.pos.x, s.pos.y);
        let (right, bottom) = (s.pos.x + s.size.x, s.pos.y + s.size.y);
        let i = ctx.insets;
        if p.x < left - i.left || p.x > right + i.right || p.y < top - i.top || p.y > bottom + i.bottom { return None; }
        let clear = |edge: Edge| !exclusions.excludes(p, edge);
        if p.y >= bottom - m.bottom_band { return clear(Edge::Bottom).then_some(Origin::Bottom); }
        if p.y <= top + m.top_band {
            let side = if p.x < left + s.size.x * 0.5 { ShadeSide::Notifications } else { ShadeSide::Controls };
            return clear(Edge::Top).then_some(Origin::Top(side));
        }
        if p.x <= left + m.edge_band { return clear(Edge::Left).then_some(Origin::Side(Edge::Left)); }
        if p.x >= right - m.edge_band { return clear(Edge::Right).then_some(Origin::Side(Edge::Right)); }
        if !ctx.body { return None; }
        match ctx.phone {
            PhoneScreen::Home => {
                let column = s.size.x * 0.25;
                if p.x < left + column { Some(Origin::Column(ShadeSide::Notifications)) }
                else if p.x > right - column { Some(Origin::Column(ShadeSide::Controls)) }
                else { Some(Origin::Body) }
            }
            // The App Library's body is its own: a drag scrolls the grid and
            // stretches past its ends; nothing there closes it.
            PhoneScreen::Drawer => None,
            _ => None,
        }
    }

    fn classify(origin: Origin, d: Vec2d) -> Option<GestureKind> {
        let (ax, ay) = (d.x.abs(), d.y.abs());
        let dir = if d.x < 0.0 { Dir::Left } else { Dir::Right };
        match origin {
            Origin::Bottom => {
                if ax > ay * 1.5 { Some(GestureKind::QuickSwitch(dir)) }
                else if d.y < 0.0 && ay >= ax { Some(GestureKind::HomeUp) }
                else { None }
            }
            Origin::Top(side) => (d.y > 0.0 && ay >= ax).then_some(GestureKind::Shade(side)),
            Origin::Side(edge) => {
                let inward = if edge == Edge::Left { d.x } else { -d.x };
                (inward > 0.0 && inward >= ay * 0.8).then_some(GestureKind::Back)
            }
            Origin::Body => {
                if ax > ay * 1.2 { Some(GestureKind::Page(dir)) }
                else if d.y > 0.0 && ay > ax * 1.2 { Some(GestureKind::HomeSearch) }
                else { None }
            }
            Origin::Column(side) => {
                if ax > ay * 1.2 { Some(GestureKind::Page(dir)) }
                else if d.y > 0.0 && ay > ax * 1.2 { Some(GestureKind::Shade(side)) }
                else { None }
            }

        }
    }

    /// How far along (of `commit_distance`) a lifted finger must be for the
    /// gesture to commit rather than cancel. A pull — the shade, search, the
    /// library closing — commits from well under half way: a finger pulls
    /// shorter than it swipes, and the surface it opens keeps following it
    /// to the full distance anyway. Navigation swipes still need the whole
    /// distance (or a flick).
    fn commit_fraction(kind: GestureKind) -> f64 {
        match kind { GestureKind::Shade(_) | GestureKind::HomeSearch => 0.4, _ => 1.0 }
    }
    /// The component of `v` (a displacement or a velocity) that advances
    /// the gesture, in points.
    fn along(kind: GestureKind, origin: Origin, v: Vec2d) -> f64 {
        match kind {
            GestureKind::HomeUp | GestureKind::Switcher => -v.y,
            GestureKind::QuickSwitch(dir) | GestureKind::Page(dir) => if dir == Dir::Left { -v.x } else { v.x },
            GestureKind::Back => if origin == Origin::Side(Edge::Right) { -v.x } else { v.x },
            GestureKind::Shade(_) | GestureKind::HomeSearch => v.y,
        }
    }
    fn progress(kind: GestureKind, origin: Origin, delta: Vec2d, commit: f64) -> f64 {
        (Self::along(kind, origin, delta) / commit).clamp(0.0, 1.0)
    }
    fn live(kind: GestureKind, origin: Origin, progress: f64, held: bool) -> ShellGesture {
        match kind {
            GestureKind::HomeUp | GestureKind::Switcher => ShellGesture::HomeUp { progress, held },
            GestureKind::QuickSwitch(dir) => ShellGesture::QuickSwitch { dir, progress },
            GestureKind::Back => ShellGesture::Back { edge: if let Origin::Side(e) = origin { e } else { Edge::Left }, progress },
            GestureKind::Shade(side) => ShellGesture::ShadePull { side, progress },
            GestureKind::Page(dir) => ShellGesture::PageSwipe { dir, progress },
            GestureKind::HomeSearch => ShellGesture::HomeSearch { progress },
        }
    }
    fn sample(t: &mut Track, p: Vec2d, time: f64) {
        t.last = p;
        if (p - t.still_anchor).length() > STILL { t.still_anchor = p; t.still_since = time; t.still_tick = None; }
        if t.samples.len() == SAMPLES { t.samples.pop_front(); }
        t.samples.push_back((p, time));
    }
    /// Points per second over the newest `VELOCITY_WINDOW` of samples.
    fn velocity(t: &Track) -> Vec2d {
        let Some(&(newest, now)) = t.samples.back() else { return dvec2(0.0, 0.0) };
        let Some(&(oldest, then)) = t.samples.iter().find(|(_, time)| now - *time <= VELOCITY_WINDOW) else { return dvec2(0.0, 0.0) };
        let dt = now - then;
        if dt < 1e-4 { return dvec2(0.0, 0.0); }
        (newest - oldest) / dt
    }
    /// True when this call turned the home swipe into the held switcher.
    fn check_hold(t: &mut Track, kind: GestureKind, progress: f64, time: f64, hold_time: f64) -> bool {
        if kind != GestureKind::HomeUp || t.held || progress < HOLD_NEAR || time - t.still_since < hold_time { return false; }
        t.held = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use FingerPhase::*;

    fn screen() -> Rect { Rect { pos: dvec2(0.0, 0.0), size: dvec2(412.0, 892.0) } }
    fn ctx(phone: PhoneScreen) -> GestureContext { GestureContext { screen: screen(), insets: SafeInsets::default(), phone, body: matches!(phone, PhoneScreen::Home | PhoneScreen::Drawer) } }
    /// Feed a finger path; each step is (phase, x, y, time).
    fn drive(rec: &mut GestureRecognizer, ctx: &GestureContext, ex: &ExclusionZones, steps: &[(FingerPhase, f64, f64, f64)]) -> Vec<Option<ShellGesture>> {
        steps.iter().map(|&(phase, x, y, t)| rec.feed(phase, dvec2(x, y), t, ctx, ex)).collect()
    }
    /// A straight drag from `a` to `b` over `secs`, in `n` moves, lifting at the end.
    fn swipe(a: (f64, f64), b: (f64, f64), secs: f64, n: usize) -> Vec<(FingerPhase, f64, f64, f64)> {
        let mut steps = vec![(Down, a.0, a.1, 1.0)];
        for i in 1..=n {
            let k = i as f64 / n as f64;
            steps.push((Move, a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k, 1.0 + secs * k));
        }
        steps.push((Up, b.0, b.1, 1.0 + secs));
        steps
    }
    fn last(out: &[Option<ShellGesture>]) -> ShellGesture { out.last().copied().flatten().expect("a terminal gesture") }

    #[test]
    fn home_swipe_commits() {
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::App), &ExclusionZones::default(), &swipe((200.0, 880.0), (200.0, 700.0), 0.2, 6));
        assert!(matches!(out[3], Some(ShellGesture::HomeUp { progress, held: false }) if progress > 0.5 && progress < 1.0), "{:?}", out[3]);
        assert_eq!(last(&out), ShellGesture::Commit(GestureKind::HomeUp));
        assert!(!rec.active());
    }
    #[test]
    fn short_swipe_cancels() {
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::App), &ExclusionZones::default(), &swipe((200.0, 880.0), (200.0, 840.0), 0.6, 6));
        assert_eq!(last(&out), ShellGesture::Cancel(GestureKind::HomeUp));
    }
    #[test]
    fn hold_produces_held_and_commits_the_switcher() {
        let mut rec = GestureRecognizer::default();
        let ctx = ctx(PhoneScreen::App);
        let ex = ExclusionZones::default();
        drive(&mut rec, &ctx, &ex, &[(Down, 200.0, 880.0, 1.0), (Move, 200.0, 840.0, 1.05), (Move, 200.0, 780.0, 1.1)]);
        assert!(matches!(rec.current(), Some(ShellGesture::HomeUp { held: false, .. })));
        // The frame clock is another epoch than the finger's (Android's
        // touches carry the event uptime): the pause counts from the first
        // tick after the last move.
        assert_eq!(rec.tick(900.0), None, "not held yet");
        assert_eq!(rec.tick(900.2), None, "not held yet");
        let held = rec.tick(900.3).expect("the pause turns the swipe into the switcher");
        assert!(matches!(held, ShellGesture::HomeUp { held: true, .. }), "{held:?}");
        assert_eq!(rec.tick(900.4), None, "reported once");
        let up = rec.feed(Up, dvec2(200.0, 780.0), 1.7, &ctx, &ex);
        assert_eq!(up, Some(ShellGesture::Commit(GestureKind::Switcher)));
    }
    #[test]
    fn a_fast_flick_along_the_bottom_is_quick_switch() {
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::App), &ExclusionZones::default(), &swipe((100.0, 882.0), (200.0, 884.0), 0.06, 4));
        assert!(matches!(out[2], Some(ShellGesture::QuickSwitch { dir: Dir::Right, .. })), "{:?}", out[2]);
        assert_eq!(last(&out), ShellGesture::Commit(GestureKind::QuickSwitch(Dir::Right)), "100pt in 60ms is a flick even short of the commit distance");
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::App), &ExclusionZones::default(), &swipe((300.0, 882.0), (240.0, 884.0), 1.0, 4));
        assert_eq!(last(&out), ShellGesture::Cancel(GestureKind::QuickSwitch(Dir::Left)), "a slow short slide cancels");
    }
    #[test]
    fn a_left_edge_drag_is_back() {
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::App), &ExclusionZones::default(), &swipe((6.0, 400.0), (160.0, 404.0), 0.3, 5));
        assert!(matches!(out[2], Some(ShellGesture::Back { edge: Edge::Left, progress }) if progress > 0.0), "{:?}", out[2]);
        assert_eq!(last(&out), ShellGesture::Commit(GestureKind::Back));
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::App), &ExclusionZones::default(), &swipe((408.0, 400.0), (300.0, 404.0), 0.3, 5));
        assert!(matches!(out[2], Some(ShellGesture::Back { edge: Edge::Right, .. })), "{:?}", out[2]);
    }
    #[test]
    fn top_left_pulls_notifications_and_top_right_controls() {
        for (x, side) in [(100.0, ShadeSide::Notifications), (350.0, ShadeSide::Controls)] {
            let mut rec = GestureRecognizer::default();
            let out = drive(&mut rec, &ctx(PhoneScreen::Home), &ExclusionZones::default(), &swipe((x, 8.0), (x, 300.0), 0.3, 5));
            assert!(matches!(out[2], Some(ShellGesture::ShadePull { side: s, .. }) if s == side), "{:?}", out[2]);
            assert_eq!(last(&out), ShellGesture::Commit(GestureKind::Shade(side)));
        }
    }
    #[test]
    fn an_excluded_rect_suppresses_back() {
        let mut ex = ExclusionZones::default();
        ex.add(Rect { pos: dvec2(0.0, 42.0), size: dvec2(412.0, 826.0) }, [false, false, true, true]);
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::App), &ex, &swipe((6.0, 400.0), (160.0, 404.0), 0.3, 5));
        assert!(out.iter().all(|g| g.is_none()), "the map keeps its edge: {out:?}");
        assert!(!rec.active());
        // The bottom band is not part of that zone: home still works.
        let out = drive(&mut rec, &ctx(PhoneScreen::App), &ex, &swipe((200.0, 880.0), (200.0, 700.0), 0.2, 6));
        assert_eq!(last(&out), ShellGesture::Commit(GestureKind::HomeUp));
    }
    #[test]
    fn the_keyboard_owns_the_bottom_band() {
        let mut ex = ExclusionZones::default();
        // The keyboard plus the navigation bar under it, as the shell registers it.
        ex.add(Rect { pos: dvec2(0.0, 576.0), size: dvec2(412.0, 316.0) }, [false, true, false, false]);
        let mut rec = GestureRecognizer::default();
        rec.feed(Down, dvec2(200.0, 880.0), 1.0, &ctx(PhoneScreen::App), &ex);
        assert!(!rec.active(), "a key at the bottom of the keyboard is a key, not home");
    }
    #[test]
    fn a_home_horizontal_drag_is_a_page_swipe() {
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::Home), &ExclusionZones::default(), &swipe((300.0, 400.0), (120.0, 410.0), 0.3, 5));
        assert!(matches!(out[2], Some(ShellGesture::PageSwipe { dir: Dir::Left, .. })), "{:?}", out[2]);
        assert_eq!(last(&out), ShellGesture::Commit(GestureKind::Page(Dir::Left)));
        // The same drag over an open app belongs to the app.
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::App), &ExclusionZones::default(), &swipe((300.0, 400.0), (120.0, 410.0), 0.3, 5));
        assert!(out.iter().all(|g| g.is_none()) && !rec.active());
    }
    #[test]
    fn a_home_downward_drag_is_search_and_upward_is_nothing() {
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::Home), &ExclusionZones::default(), &swipe((200.0, 300.0), (204.0, 500.0), 0.3, 5));
        assert!(matches!(out[2], Some(ShellGesture::HomeSearch { .. })), "{:?}", out[2]);
        assert_eq!(last(&out), ShellGesture::Commit(GestureKind::HomeSearch));
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::Home), &ExclusionZones::default(), &swipe((200.0, 500.0), (204.0, 300.0), 0.3, 5));
        assert!(out.iter().all(|g| g.is_none()), "an upward drag on the home page is the drawer's, not a shell gesture: {out:?}");
    }
    #[test]
    fn a_downward_drag_in_a_home_column_pulls_that_side_of_the_shade() {
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::Home), &ExclusionZones::default(), &swipe((380.0, 300.0), (384.0, 500.0), 0.3, 5));
        assert!(matches!(out[2], Some(ShellGesture::ShadePull { side: ShadeSide::Controls, .. })), "{:?}", out[2]);
        assert_eq!(last(&out), ShellGesture::Commit(GestureKind::Shade(ShadeSide::Controls)));
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::Home), &ExclusionZones::default(), &swipe((60.0, 300.0), (64.0, 500.0), 0.3, 5));
        assert!(matches!(out[2], Some(ShellGesture::ShadePull { side: ShadeSide::Notifications, .. })), "{:?}", out[2]);
        // A horizontal drag from a column is still a page swipe.
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::Home), &ExclusionZones::default(), &swipe((380.0, 400.0), (200.0, 410.0), 0.3, 5));
        assert!(matches!(out[2], Some(ShellGesture::PageSwipe { dir: Dir::Left, .. })), "{:?}", out[2]);
    }
    #[test]
    fn a_short_pull_commits_but_a_short_home_swipe_does_not() {
        // 60 points down, slowly: half the commit distance, no flick.
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::Home), &ExclusionZones::default(), &swipe((200.0, 300.0), (203.0, 360.0), 0.5, 6));
        assert_eq!(last(&out), ShellGesture::Commit(GestureKind::HomeSearch), "{out:?}");
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::Home), &ExclusionZones::default(), &swipe((380.0, 300.0), (383.0, 360.0), 0.5, 6));
        assert_eq!(last(&out), ShellGesture::Commit(GestureKind::Shade(ShadeSide::Controls)), "{out:?}");
        // In the library the same pull is the grid's own scroll, not a gesture.
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::Drawer), &ExclusionZones::default(), &swipe((200.0, 300.0), (203.0, 360.0), 0.5, 6));
        assert!(out.iter().all(|g| g.is_none()), "{out:?}");
        // The same 60 points up from the bottom band is not a home swipe yet.
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::App), &ExclusionZones::default(), &swipe((200.0, 880.0), (203.0, 820.0), 0.5, 6));
        assert_eq!(last(&out), ShellGesture::Cancel(GestureKind::HomeUp), "{out:?}");
    }
    #[test]
    fn nothing_in_the_library_body_is_a_shell_gesture() {
        // A downward drag stays the grid's (it scrolls, and stretches at the top).
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::Drawer), &ExclusionZones::default(), &swipe((200.0, 300.0), (204.0, 500.0), 0.3, 5));
        assert!(out.iter().all(|g| g.is_none()), "a library pull is not a gesture: {out:?}");
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::Drawer), &ExclusionZones::default(), &swipe((300.0, 400.0), (120.0, 410.0), 0.3, 5));
        assert!(out.iter().all(|g| g.is_none()), "no pages in the library: {out:?}");
        // Scrolling search results is the library's: the body is not offered.
        let mut rec = GestureRecognizer::default();
        let ctx = GestureContext { body: false, ..ctx(PhoneScreen::Drawer) };
        let out = drive(&mut rec, &ctx, &ExclusionZones::default(), &swipe((200.0, 300.0), (204.0, 500.0), 0.3, 5));
        assert!(out.iter().all(|g| g.is_none()), "{out:?}");
    }
    #[test]
    fn a_tap_in_a_band_is_not_a_gesture_and_a_finger_in_the_nav_bar_is_the_bottom_band() {
        let mut rec = GestureRecognizer::default();
        let out = drive(&mut rec, &ctx(PhoneScreen::App), &ExclusionZones::default(), &[(Down, 200.0, 880.0, 1.0), (Move, 202.0, 878.0, 1.05), (Up, 202.0, 878.0, 1.1)]);
        assert!(out.iter().all(|g| g.is_none()), "{out:?}");
        let ctx = GestureContext { insets: SafeInsets { bottom: 20.0, ..Default::default() }, ..ctx(PhoneScreen::App) };
        let out = drive(&mut rec, &ctx, &ExclusionZones::default(), &swipe((200.0, 905.0), (200.0, 700.0), 0.2, 6));
        assert_eq!(last(&out), ShellGesture::Commit(GestureKind::HomeUp));
        assert_eq!(rec.feed(Down, dvec2(200.0, 930.0), 3.0, &ctx, &ExclusionZones::default()), None);
        assert!(!rec.active(), "below the navigation bar is off the glass");
    }
    #[test]
    fn cancel_reports_the_gesture_in_progress() {
        let mut rec = GestureRecognizer::default();
        drive(&mut rec, &ctx(PhoneScreen::App), &ExclusionZones::default(), &[(Down, 6.0, 400.0, 1.0), (Move, 80.0, 402.0, 1.1)]);
        assert_eq!(rec.cancel(), Some(ShellGesture::Cancel(GestureKind::Back)));
        assert_eq!(rec.cancel(), None);
    }
}
