//! Guidance: the navigation library's session around a route, fed by live
//! fixes or by the simulated drive, one `NavTick` per position — where the
//! puck goes, how the camera turns, what the banner says. After the
//! framework's `apps/route/src/nav.rs`, with one leg, the clock passed in
//! (so it tests without a platform) and the banner read off the step list.

use crate::geo::{bearing_deg, LonLat};
use crate::routing::{Arrow, Directions};
use makepad_map_nav::geo::bearing_delta_deg;
use makepad_map_nav::graph::Route;
use makepad_map_nav::nav::{ManeuverKind, NavSession, NavState};

/// A preview runs the route this many times faster than the drive would be…
const SIM_SPEED_MULT: f64 = 6.0;
/// …but always lasts between these, whatever the route: a walk across town
/// is not an eighteen-minute preview, a hop next door not a blink.
const PREVIEW_SECONDS: std::ops::RangeInclusive<f64> = 20.0..=90.0;
/// How far ahead on the line the travel bearing is taken.
const LOOK_AHEAD_M: f64 = 12.0;
/// How fast the camera turns onto the travel bearing: 1/e of the way per
/// this fraction of a second.
const ROTATION_EASE_PER_S: f64 = 3.0;
/// Until the drive has gone this far, the route's first turn stands. OSRM
/// often sets off a metre or two before a corner, inside the session's own
/// look-ahead, which would pass over that turn without ever naming it.
const START_ZONE_M: f64 = 10.0;
/// Still off the route this long after asking for a new one (the request
/// failed, or the new route is no better): ask again.
const REROUTE_RETRY_S: f64 = 15.0;

/// What one position means for the screen.
#[derive(Clone, Debug)]
pub struct NavTick {
    /// Where the puck goes: on the line while on the route, the raw fix off it.
    pub position: LonLat,
    pub heading: Option<f64>,
    /// The heading-up map rotation, eased.
    pub rotation: f64,
    /// Line points before this are behind the puck.
    pub progress_index: usize,
    pub state: NavState,
    pub arrow: Option<Arrow>,
    pub banner: String,
    pub banner_distance_m: f64,
    pub remaining_m: f64,
    pub remaining_s: f64,
    /// Ask for a new route from `position` now.
    pub needs_reroute: bool,
}

pub struct ActiveNav {
    session: NavSession,
    directions: Directions,
    simulate: bool,
    sim_progress_m: f64,
    /// Seconds of guidance so far: the session's monotonic clock.
    clock_s: f64,
    rotation: f64,
    reroute_asked_at: Option<f64>,
}

impl ActiveNav {
    pub fn new(directions: Directions, simulate: bool) -> Self {
        // The map starts turned the way the route sets off.
        let rotation = travel_bearing(&directions.route, 0.0).unwrap_or(0.0);
        ActiveNav {
            session: NavSession::new(directions.route.clone()),
            directions,
            simulate,
            sim_progress_m: 0.0,
            clock_s: 0.0,
            rotation,
            reroute_asked_at: None,
        }
    }

    pub fn simulate(&self) -> bool {
        self.simulate
    }

    pub fn directions(&self) -> &Directions {
        &self.directions
    }

    /// Advance the simulated drive by `dt` seconds.
    pub fn tick_sim(&mut self, dt: f64) -> NavTick {
        // A stalled frame does not jump the puck down the road.
        let dt = dt.clamp(0.0, 0.5);
        let route = &self.directions.route;
        let preview_s = (route.duration_s / SIM_SPEED_MULT)
            .clamp(*PREVIEW_SECONDS.start(), *PREVIEW_SECONDS.end());
        self.sim_progress_m =
            (self.sim_progress_m + route.length_m / preview_s * dt).min(route.length_m);
        let pos = point_at(route, self.sim_progress_m);
        let heading = travel_bearing(route, self.sim_progress_m);
        self.feed(pos, heading, dt)
    }

    /// A position, real or simulated, `dt` seconds after the last.
    pub fn feed(&mut self, pos: LonLat, heading: Option<f64>, dt: f64) -> NavTick {
        let dt = dt.max(0.0);
        self.clock_s += dt;
        let status = self.session.update(pos, self.clock_s);
        let route = &self.directions.route;

        // Heading-up: ease onto the compass when the fix has one, else onto
        // the street being driven; the shortest way round.
        if let Some(target) = heading.or_else(|| travel_bearing(route, status.progress_m)) {
            let blend = 1.0 - (-dt * ROTATION_EASE_PER_S).exp();
            self.rotation = (self.rotation + bearing_delta_deg(self.rotation, target) * blend)
                .rem_euclid(360.0);
        }

        // One ask per episode off the route, and another only if it lasts.
        if status.state == NavState::Navigating {
            self.reroute_asked_at = None;
        }
        let may_ask = self
            .reroute_asked_at
            .map_or(true, |at| self.clock_s - at >= REROUTE_RETRY_S);
        let needs_reroute = status.needs_reroute && !self.simulate && may_ask;
        if needs_reroute {
            self.reroute_asked_at = Some(self.clock_s);
        }

        // At the very start the first turn, however close; then the
        // session's pick.
        let first_turn = route
            .maneuvers
            .iter()
            .position(|m| m.kind != ManeuverKind::Depart);
        let next = if status.progress_m < START_ZONE_M {
            first_turn.filter(|&m| route.maneuvers[m].dist_m >= status.progress_m)
        } else {
            None
        }
        .or(status.next_maneuver);
        let banner_distance_m = next
            .map(|m| (route.maneuvers[m].dist_m - status.progress_m).max(0.0))
            .unwrap_or(status.remaining_m);
        let (arrow, banner) = if status.state == NavState::Arrived {
            (Some(Arrow::Arrive), "You have arrived".to_string())
        } else {
            // The turn's line in the list says it better than its kind does.
            let step = next
                .and_then(|m| self.directions.maneuver_steps.get(m))
                .and_then(|&s| self.directions.steps.get(s));
            (
                step.map(|s| s.arrow),
                step.map(|s| s.text.clone()).unwrap_or_default(),
            )
        };
        NavTick {
            position: status.matched,
            heading,
            rotation: self.rotation,
            progress_index: route.cum_dist_m.partition_point(|&c| c < status.progress_m),
            state: status.state,
            arrow,
            banner,
            banner_distance_m,
            remaining_m: status.remaining_m,
            remaining_s: status.remaining_s,
            needs_reroute,
        }
    }

    /// The reroute's answer: guidance starts over on the new route.
    pub fn replace_route(&mut self, directions: Directions) {
        self.session = NavSession::new(directions.route.clone());
        self.directions = directions;
        self.sim_progress_m = 0.0;
        self.reroute_asked_at = None;
    }
}

/// The point `dist_m` along the route's line.
pub fn point_at(route: &Route, dist_m: f64) -> LonLat {
    let (points, cum) = (&route.points, &route.cum_dist_m);
    let Some(last) = points.last() else {
        return LonLat::new(0.0, 0.0);
    };
    let next = cum.partition_point(|&c| c < dist_m);
    if next == 0 {
        return points[0];
    }
    if next >= points.len() {
        return *last;
    }
    let (a, b) = (points[next - 1], points[next]);
    let t = (dist_m - cum[next - 1]) / (cum[next] - cum[next - 1]).max(1e-9);
    LonLat::new(a.lon + (b.lon - a.lon) * t, a.lat + (b.lat - a.lat) * t)
}

/// The bearing of the line a little ahead of `dist_m`; `None` at its end,
/// where there is nothing ahead to point at.
fn travel_bearing(route: &Route, dist_m: f64) -> Option<f64> {
    (dist_m + 1.0 < route.length_m).then(|| {
        bearing_deg(
            point_at(route, dist_m),
            point_at(route, (dist_m + LOOK_AHEAD_M).min(route.length_m)),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{cumulative_distances, haversine_m};
    use crate::routing::Step;
    use makepad_map_nav::graph::TravelMode;
    use makepad_map_nav::nav::Maneuver;

    /// Roughly 50 m of latitude, and of longitude at this latitude.
    const STEP_LAT: f64 = 0.00045;
    const STEP_LON: f64 = 0.000566;
    const ORIGIN: LonLat = LonLat {
        lon: -121.9,
        lat: 37.3,
    };

    /// 500 m north, a right turn onto Elm Street, 500 m east: a vertex every
    /// 50 m, a hundred seconds to drive.
    fn l_shaped() -> Directions {
        let mut points: Vec<LonLat> = (0..=10)
            .map(|i| LonLat::new(ORIGIN.lon, ORIGIN.lat + STEP_LAT * i as f64))
            .collect();
        let corner = *points.last().unwrap();
        points.extend((1..=10).map(|i| LonLat::new(corner.lon + STEP_LON * i as f64, corner.lat)));
        let cum_dist_m = cumulative_distances(&points).unwrap();
        let length_m = *cum_dist_m.last().unwrap();
        let maneuver = |kind, index: usize, name: &str| Maneuver {
            kind,
            at: points[index],
            name: name.into(),
            dist_m: cum_dist_m[index],
            point_index: index,
        };
        let maneuvers = vec![
            maneuver(ManeuverKind::Depart, 0, "Oak Street"),
            maneuver(ManeuverKind::TurnRight, 10, "Elm Street"),
            maneuver(ManeuverKind::Arrive, 20, "Elm Street"),
        ];
        let step = |arrow, text: &str, distance_m| Step {
            arrow,
            text: text.into(),
            distance_m,
        };
        Directions {
            route: Route {
                mode: TravelMode::Car,
                points,
                cum_dist_m,
                length_m,
                duration_s: 100.0,
                maneuvers,
            },
            steps: vec![
                step(Arrow::Depart, "Head north on Oak Street", 500.0),
                step(Arrow::Right, "Turn right onto Elm Street", 500.0),
                step(Arrow::Arrive, "Arrive at your destination", 0.0),
            ],
            maneuver_steps: vec![0, 1, 2],
        }
    }

    #[test]
    fn a_point_along_the_line() {
        let route = l_shaped().route;
        assert_eq!(point_at(&route, -5.0), route.points[0]);
        assert_eq!(point_at(&route, 1e9), *route.points.last().unwrap());
        // Halfway up the first street.
        let halfway = point_at(&route, 250.0);
        assert!(haversine_m(halfway, route.points[5]) < 1.0);
    }

    #[test]
    fn the_preview_drives_to_the_end_and_never_backwards() {
        let mut nav = ActiveNav::new(l_shaped(), true);
        let mut last_index = 0;
        let mut ticks = 0;
        let arrived = loop {
            let tick = nav.tick_sim(0.25);
            assert!(tick.progress_index >= last_index);
            assert!(!tick.needs_reroute, "a preview never leaves its route");
            last_index = tick.progress_index;
            ticks += 1;
            if tick.state == NavState::Arrived || ticks > 1_000 {
                break tick;
            }
        };
        assert_eq!(arrived.state, NavState::Arrived);
        assert_eq!(arrived.banner, "You have arrived");
        assert_eq!(arrived.arrow, Some(Arrow::Arrive));
        // A hundred-second drive previews in the floor of twenty.
        assert!(
            (20.0 / 0.25 - 12.0..=20.0 / 0.25 + 2.0).contains(&(ticks as f64)),
            "{ticks} ticks"
        );
    }

    /// The L, but setting off two metres before a first corner, as OSRM's
    /// routes often do: a left onto Oak Street, then the L as before.
    fn corner_at_the_start() -> Directions {
        let mut directions = l_shaped();
        let first = directions.route.points[0];
        let before = LonLat::new(first.lon - STEP_LON * 0.04, first.lat);
        directions.route.points.insert(0, before);
        directions.route.cum_dist_m = cumulative_distances(&directions.route.points).unwrap();
        directions.route.length_m = *directions.route.cum_dist_m.last().unwrap();
        let cum = directions.route.cum_dist_m.clone();
        for maneuver in directions.route.maneuvers.iter_mut().skip(1) {
            maneuver.point_index += 1;
            maneuver.dist_m = cum[maneuver.point_index];
        }
        let corner = Maneuver {
            kind: ManeuverKind::TurnLeft,
            at: first,
            name: "Oak Street".into(),
            dist_m: cum[1],
            point_index: 1,
        };
        directions.route.maneuvers.insert(1, corner);
        directions.steps.insert(
            1,
            Step {
                arrow: Arrow::Left,
                text: "Turn left onto Oak Street".into(),
                distance_m: 500.0,
            },
        );
        directions.maneuver_steps = vec![0, 1, 2, 3];
        directions
    }

    #[test]
    fn a_turn_at_the_very_start_is_announced_before_the_next_one() {
        let directions = corner_at_the_start();
        assert!(
            directions.route.maneuvers[1].dist_m < 3.0,
            "inside the session's look-ahead"
        );
        let mut nav = ActiveNav::new(directions.clone(), false);
        let standing = nav.feed(directions.route.points[0], None, 1.0);
        assert_eq!(standing.banner, "Turn left onto Oak Street");
        assert_eq!(standing.arrow, Some(Arrow::Left));
        assert!(standing.banner_distance_m < 3.0);
        // Round the corner and up the street: the L's own turn is next.
        let tick = nav.feed(point_at(&directions.route, 60.0), None, 1.0);
        assert_eq!(tick.banner, "Turn right onto Elm Street");
    }

    #[test]
    fn the_banner_names_the_next_turn_and_counts_down_to_it() {
        let mut nav = ActiveNav::new(l_shaped(), true);
        let first = nav.tick_sim(0.25);
        assert_eq!(first.banner, "Turn right onto Elm Street");
        assert_eq!(first.arrow, Some(Arrow::Right));
        assert!(
            first.banner_distance_m < 500.0 && first.banner_distance_m > 400.0,
            "{}",
            first.banner_distance_m
        );
        let second = nav.tick_sim(0.25);
        assert!(second.banner_distance_m < first.banner_distance_m);
        assert!(second.remaining_m < first.remaining_m && second.remaining_s < first.remaining_s);
        // Past the corner the banner moves on to the arrival.
        let after = (0..60)
            .map(|_| nav.tick_sim(0.25))
            .find(|tick| tick.arrow == Some(Arrow::Arrive))
            .unwrap();
        assert_eq!(after.banner, "Arrive at your destination");
    }

    #[test]
    fn a_live_drive_turns_the_map_onto_the_street_even_without_a_compass() {
        let directions = l_shaped();
        let route = directions.route.clone();
        let mut nav = ActiveNav::new(directions, false);
        // Up Oak Street: north is up. 350 is as good as 10.
        let mut tick = nav.feed(point_at(&route, 100.0), None, 1.0);
        for _ in 0..3 {
            tick = nav.feed(point_at(&route, 150.0), None, 1.0);
        }
        assert!(
            bearing_delta_deg(tick.rotation, 0.0).abs() < 2.0,
            "{}",
            tick.rotation
        );
        // Round the corner and along Elm Street, in hops the session's
        // matching window follows: east is up.
        for along in [350.0, 550.0, 750.0, 760.0, 770.0, 780.0] {
            tick = nav.feed(point_at(&route, along), None, 1.0);
        }
        assert!(
            bearing_delta_deg(tick.rotation, 90.0).abs() < 2.0,
            "{}",
            tick.rotation
        );
        // A compass, when there is one, wins.
        for _ in 0..4 {
            tick = nav.feed(point_at(&route, 800.0), Some(80.0), 1.0);
        }
        assert!(
            bearing_delta_deg(tick.rotation, 80.0).abs() < 2.0,
            "{}",
            tick.rotation
        );
    }

    #[test]
    fn off_the_route_asks_for_a_new_one_once_then_again_only_after_a_while() {
        let directions = l_shaped();
        let on_route = point_at(&directions.route, 100.0);
        let astray = LonLat::new(on_route.lon - 2.0 * STEP_LON, on_route.lat);
        let mut nav = ActiveNav::new(directions, false);
        assert!(!nav.feed(on_route, None, 1.0).needs_reroute);
        // A hundred metres west of Oak Street, second after second.
        let asks: Vec<bool> = (0..30)
            .map(|_| nav.feed(astray, None, 1.0).needs_reroute)
            .collect();
        let asked_at: Vec<usize> = asks
            .iter()
            .enumerate()
            .filter(|(_, ask)| **ask)
            .map(|(i, _)| i)
            .collect();
        // Once after the session's grace, once more after the retry interval.
        assert_eq!(asked_at.len(), 2, "{asked_at:?}");
        assert!(asked_at[0] >= 4 && asked_at[0] <= 6, "{asked_at:?}");
        assert!(
            asked_at[1] - asked_at[0] >= REROUTE_RETRY_S as usize,
            "{asked_at:?}"
        );
        // Off the route the puck is where the fix is.
        let tick = nav.feed(astray, None, 1.0);
        assert_eq!(tick.state, NavState::OffRoute);
        assert_eq!(tick.position, astray);
    }

    #[test]
    fn back_on_the_route_forgets_the_asking_and_a_new_route_starts_over() {
        let directions = l_shaped();
        let route = directions.route.clone();
        let on_route = point_at(&route, 300.0);
        let astray = LonLat::new(on_route.lon - 2.0 * STEP_LON, on_route.lat);
        let mut nav = ActiveNav::new(directions, false);
        nav.feed(on_route, None, 1.0);
        assert!((0..8).any(|_| nav.feed(astray, None, 1.0).needs_reroute));
        // The reroute's answer: progress starts over on it.
        nav.replace_route(l_shaped());
        let tick = nav.feed(point_at(&route, 20.0), None, 1.0);
        assert_eq!(tick.state, NavState::Navigating);
        assert!(tick.progress_index <= 1, "{}", tick.progress_index);
        // Astray again: a fresh episode asks again, without the retry wait.
        assert!((0..8).any(|_| nav.feed(astray, None, 1.0).needs_reroute));
    }
}
