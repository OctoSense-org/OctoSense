//! Routes from OSRM (routing.openstreetmap.de, a server per profile, no
//! key), as the navigation library's `Route` — so its session map-matches
//! them — plus the step list the sheet shows.

use crate::geo::{cumulative_distances, decode_polyline5, haversine_m, LonLat};
use makepad_map_nav::graph::{Route, TravelMode};
use makepad_map_nav::nav::{Maneuver, ManeuverKind};
use makepad_widgets::makepad_micro_serde::*;

const ROUTING: &str = "https://routing.openstreetmap.de";
/// A maneuver sits on the line's own vertex, give or take the encoding's
/// fifth decimal.
const ON_VERTEX_M: f64 = 3.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    Car,
    Walk,
    Bike,
}

impl Mode {
    /// In the order the tabs show them.
    pub const ALL: [Mode; 3] = [Mode::Car, Mode::Walk, Mode::Bike];

    pub fn index(self) -> usize {
        match self {
            Mode::Car => 0,
            Mode::Walk => 1,
            Mode::Bike => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Car => "Drive",
            Mode::Walk => "Walk",
            Mode::Bike => "Bike",
        }
    }

    pub fn travel_mode(self) -> TravelMode {
        match self {
            Mode::Car => TravelMode::Car,
            Mode::Walk => TravelMode::Foot,
            Mode::Bike => TravelMode::Bike,
        }
    }

    /// The server that routes this mode.
    fn profile(self) -> &'static str {
        match self {
            Mode::Car => "routed-car",
            Mode::Walk => "routed-foot",
            Mode::Bike => "routed-bike",
        }
    }
}

/// The arrow a step or the banner shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    Depart,
    Arrive,
    Straight,
    SlightLeft,
    Left,
    SharpLeft,
    SlightRight,
    Right,
    SharpRight,
    UTurn,
    Roundabout,
}

impl Arrow {
    pub fn of(kind: ManeuverKind) -> Arrow {
        match kind {
            ManeuverKind::Depart => Arrow::Depart,
            ManeuverKind::Arrive => Arrow::Arrive,
            ManeuverKind::TurnSlightLeft => Arrow::SlightLeft,
            ManeuverKind::TurnLeft => Arrow::Left,
            ManeuverKind::TurnSharpLeft => Arrow::SharpLeft,
            ManeuverKind::TurnSlightRight => Arrow::SlightRight,
            ManeuverKind::TurnRight => Arrow::Right,
            ManeuverKind::TurnSharpRight => Arrow::SharpRight,
            ManeuverKind::UTurn => Arrow::UTurn,
            ManeuverKind::RoundaboutExit(_) => Arrow::Roundabout,
        }
    }
}

/// One line of the directions list.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub arrow: Arrow,
    /// `Turn left onto North 5th Street`.
    pub text: String,
    /// How far this step goes before the next one.
    pub distance_m: f64,
}

#[derive(Clone, Debug)]
pub struct Directions {
    pub route: Route,
    pub steps: Vec<Step>,
    /// For each of `route.maneuvers`, its line in `steps`: the banner reads
    /// the step's text, which knows more than the maneuver's kind does.
    pub maneuver_steps: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RouteError {
    /// The service answered, and there is no way between the two points.
    NoRoute,
    Malformed(String),
}

/// The route from `from` to `to` for `mode`, with its steps and whole line.
pub fn route_url(mode: Mode, from: LonLat, to: LonLat) -> String {
    // `driving` is the path's fixed word on these servers: the host picks the profile.
    format!(
        "{ROUTING}/{}/route/v1/driving/{:.6},{:.6};{:.6},{:.6}?overview=full&steps=true&geometries=polyline",
        mode.profile(),
        from.lon,
        from.lat,
        to.lon,
        to.lat
    )
}

pub fn parse(mode: Mode, json: &str) -> Result<Directions, RouteError> {
    let malformed = |what: &str| RouteError::Malformed(what.to_string());
    // Lenient: OSRM sends fields we do not model (waypoints, weights, intersections).
    let reply: Reply = DeJson::deserialize_json_lenient(json)
        .map_err(|e| RouteError::Malformed(format!("{e:?}")))?;
    match reply.code.as_deref() {
        Some("Ok") => {}
        Some("NoRoute" | "NoSegment") => return Err(RouteError::NoRoute),
        code => {
            return Err(RouteError::Malformed(format!(
                "{}: {}",
                code.unwrap_or("no code"),
                reply.message.unwrap_or_default()
            )));
        }
    }
    let wire = reply
        .routes
        .unwrap_or_default()
        .into_iter()
        .next()
        .ok_or_else(|| malformed("no routes"))?;
    let points = decode_polyline5(wire.geometry.as_deref().unwrap_or(""));
    if points.len() < 2 {
        return Err(malformed("a line of fewer than two points"));
    }
    // Our own sum over the line, not OSRM's `distance`: the session measures
    // progress along these same points.
    let cum_dist_m =
        cumulative_distances(&points).ok_or_else(|| malformed("a line too long to follow"))?;
    let length_m = cum_dist_m[cum_dist_m.len() - 1];

    let last_point = points.len() - 1;
    let wire_steps: Vec<StepWire> = wire
        .legs
        .unwrap_or_default()
        .into_iter()
        .flat_map(|leg| leg.steps.unwrap_or_default())
        .collect();
    let last_step = wire_steps.len().saturating_sub(1);
    let mut steps: Vec<Step> = Vec::new();
    let mut maneuvers: Vec<Maneuver> = Vec::new();
    let mut maneuver_steps = Vec::new();
    let mut reached = 0usize;
    for (step_index, wire_step) in wire_steps.into_iter().enumerate() {
        let distance_m = wire_step.distance.unwrap_or(0.0).max(0.0);
        let m = wire_step.maneuver.unwrap_or_default();
        let (kind_name, modifier) = (
            m.kind.as_deref().unwrap_or(""),
            m.modifier.as_deref().unwrap_or(""),
        );
        if is_hint(kind_name) {
            // Not a line of its own: its stretch belongs to the step before.
            if let Some(previous) = steps.last_mut() {
                previous.distance_m += distance_m;
            }
            continue;
        }
        let kind = maneuver_kind(
            kind_name,
            modifier,
            m.exit.unwrap_or(0.0).clamp(0.0, 255.0) as u8,
        );
        let name = wire_step.name.as_deref().map(str::trim).unwrap_or("");
        steps.push(Step {
            arrow: kind.map(Arrow::of).unwrap_or(Arrow::Straight),
            text: step_text(kind, kind_name, modifier, name, m.bearing_after),
            distance_m,
        });
        let at = match m.location.as_deref() {
            Some([lon, lat, ..]) if lon.is_finite() && lat.is_finite() => LonLat::new(*lon, *lat),
            _ => points[reached],
        };
        // Every step moves the search on, a turn or not, so a line that
        // passes a vertex twice pins a later turn to the later pass. Only
        // the first step is the line's start and only the last its end: a
        // leg's own depart or arrive in between is wherever it says.
        reached = match kind {
            Some(ManeuverKind::Depart) if step_index == 0 => 0,
            Some(ManeuverKind::Arrive) if step_index == last_step => last_point,
            _ => vertex_at(&points, reached, at),
        };
        let Some(kind) = kind else { continue };
        maneuvers.push(Maneuver {
            kind,
            at,
            name: name.to_string(),
            dist_m: cum_dist_m[reached],
            point_index: reached,
        });
        maneuver_steps.push(steps.len() - 1);
    }
    let route = Route {
        mode: mode.travel_mode(),
        points,
        cum_dist_m,
        length_m,
        duration_s: wire.duration.unwrap_or(0.0).max(0.0),
        maneuvers,
    };
    Ok(Directions {
        route,
        steps,
        maneuver_steps,
    })
}

#[derive(Clone, Debug, Default, DeJson)]
struct Reply {
    code: Option<String>,
    message: Option<String>,
    routes: Option<Vec<RouteWire>>,
}

#[derive(Clone, Debug, Default, DeJson)]
struct RouteWire {
    geometry: Option<String>,
    duration: Option<f64>,
    legs: Option<Vec<LegWire>>,
}

#[derive(Clone, Debug, Default, DeJson)]
struct LegWire {
    steps: Option<Vec<StepWire>>,
}

#[derive(Clone, Debug, Default, DeJson)]
struct StepWire {
    name: Option<String>,
    distance: Option<f64>,
    maneuver: Option<ManeuverWire>,
}

#[derive(Clone, Debug, Default, DeJson)]
struct ManeuverWire {
    #[rename(type)]
    kind: Option<String>,
    modifier: Option<String>,
    exit: Option<f64>,
    location: Option<Vec<f64>>,
    bearing_after: Option<f64>,
}

/// The line's vertex a maneuver sits on, looking forward from `from` only,
/// so a route that crosses itself never sends the indices backwards: the
/// first vertex on the spot, else the nearest one ahead.
fn vertex_at(points: &[LonLat], from: usize, at: LonLat) -> usize {
    let mut nearest = (f64::MAX, from);
    for (i, point) in points.iter().enumerate().skip(from) {
        let d = haversine_m(*point, at);
        if d <= ON_VERTEX_M {
            return i;
        }
        if d < nearest.0 {
            nearest = (d, i);
        }
    }
    nearest.1
}

/// OSRM step types that tell a driver something without being a place in
/// the list: a lane hint, a notice, the far side of a roundabout.
fn is_hint(kind: &str) -> bool {
    matches!(
        kind,
        "notification" | "use lane" | "exit roundabout" | "exit rotary"
    )
}

fn step_text(
    kind: Option<ManeuverKind>,
    kind_name: &str,
    modifier: &str,
    name: &str,
    bearing_after: Option<f64>,
) -> String {
    let onto = |base: String| {
        if name.is_empty() {
            base
        } else {
            format!("{base} onto {name}")
        }
    };
    let side = if modifier.contains("left") {
        Some("left")
    } else if modifier.contains("right") {
        Some("right")
    } else {
        None
    };
    match kind {
        Some(ManeuverKind::Depart) => {
            let heading = format!("Head {}", cardinal(bearing_after.unwrap_or(0.0)));
            if name.is_empty() {
                heading
            } else {
                format!("{heading} on {name}")
            }
        }
        Some(ManeuverKind::Arrive) => match side {
            Some(side) => format!("Arrive at your destination, on the {side}"),
            None => "Arrive at your destination".to_string(),
        },
        Some(roundabout @ ManeuverKind::RoundaboutExit(_)) => onto(roundabout.instruction()),
        Some(turn) => onto(match (kind_name, side) {
            ("fork", Some(side)) => format!("Keep {side}"),
            ("merge", _) => "Merge".to_string(),
            ("on ramp", Some(side)) => format!("Take the ramp on the {side}"),
            ("off ramp", Some(side)) => format!("Take the exit on the {side}"),
            _ => turn_text(turn).to_string(),
        }),
        None if name.is_empty() => "Continue straight".to_string(),
        None => format!("Continue onto {name}"),
    }
}

fn turn_text(kind: ManeuverKind) -> &'static str {
    match kind {
        ManeuverKind::TurnSlightLeft => "Slight left",
        ManeuverKind::TurnLeft => "Turn left",
        ManeuverKind::TurnSharpLeft => "Sharp left",
        ManeuverKind::TurnSlightRight => "Slight right",
        ManeuverKind::TurnRight => "Turn right",
        ManeuverKind::TurnSharpRight => "Sharp right",
        ManeuverKind::UTurn => "Make a U-turn",
        ManeuverKind::Depart | ManeuverKind::Arrive | ManeuverKind::RoundaboutExit(_) => "Continue",
    }
}

/// The eight-point compass name of a bearing in degrees.
fn cardinal(bearing: f64) -> &'static str {
    const NAMES: [&str; 8] = [
        "north",
        "northeast",
        "east",
        "southeast",
        "south",
        "southwest",
        "west",
        "northwest",
    ];
    NAMES[((bearing.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8]
}

/// An OSRM maneuver as a turn the session announces, or `None` when it is
/// not one: going straight on, a road changing its name, a lane hint.
pub fn maneuver_kind(kind: &str, modifier: &str, exit: u8) -> Option<ManeuverKind> {
    match kind {
        "depart" => Some(ManeuverKind::Depart),
        "arrive" => Some(ManeuverKind::Arrive),
        "roundabout" | "rotary" => Some(ManeuverKind::RoundaboutExit(exit.max(1))),
        // Wherever the road or the driver changes direction, the modifier
        // says which way; `straight` (or none) is no turn at all.
        "turn" | "end of road" | "fork" | "merge" | "on ramp" | "off ramp" | "roundabout turn"
        | "new name" | "continue" => match modifier {
            "uturn" => Some(ManeuverKind::UTurn),
            "sharp left" => Some(ManeuverKind::TurnSharpLeft),
            "left" => Some(ManeuverKind::TurnLeft),
            "slight left" => Some(ManeuverKind::TurnSlightLeft),
            "sharp right" => Some(ManeuverKind::TurnSharpRight),
            "right" => Some(ManeuverKind::TurnRight),
            "slight right" => Some(ManeuverKind::TurnSlightRight),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAR: &str = include_str!("../tests/fixtures/osrm-car.json");
    const BIKE: &str = include_str!("../tests/fixtures/osrm-bike.json");
    const FOOT: &str = include_str!("../tests/fixtures/osrm-foot.json");
    const NO_ROUTE: &str = include_str!("../tests/fixtures/osrm-noroute.json");

    #[test]
    fn every_mode_parses_into_a_route_the_session_can_follow() {
        for (mode, json) in [(Mode::Car, CAR), (Mode::Bike, BIKE), (Mode::Walk, FOOT)] {
            let directions = parse(mode, json).unwrap();
            let route = &directions.route;
            assert_eq!(route.mode, mode.travel_mode());
            assert!(route.points.len() > 10, "{mode:?}");
            assert_eq!(route.points.len(), route.cum_dist_m.len());
            // The distances run forward and end at the length.
            assert!(
                route.cum_dist_m.windows(2).all(|w| w[1] >= w[0]),
                "{mode:?}"
            );
            assert_eq!(*route.cum_dist_m.last().unwrap(), route.length_m);
            // About 7.7 km, by our own sum over the line.
            assert!(
                (7_000.0..8_500.0).contains(&route.length_m),
                "{mode:?} {}",
                route.length_m
            );
            assert!(route.duration_s > 0.0);
        }
    }

    #[test]
    fn the_modes_take_their_own_time() {
        let car = parse(Mode::Car, CAR).unwrap().route.duration_s;
        let bike = parse(Mode::Bike, BIKE).unwrap().route.duration_s;
        let foot = parse(Mode::Walk, FOOT).unwrap().route.duration_s;
        assert!(car < bike && bike < foot, "{car} {bike} {foot}");
    }

    #[test]
    fn maneuvers_run_forward_from_depart_to_arrive() {
        for (mode, json) in [(Mode::Car, CAR), (Mode::Bike, BIKE), (Mode::Walk, FOOT)] {
            let route = parse(mode, json).unwrap().route;
            let maneuvers = &route.maneuvers;
            assert_eq!(maneuvers.first().unwrap().kind, ManeuverKind::Depart);
            assert_eq!(maneuvers.first().unwrap().point_index, 0);
            assert_eq!(maneuvers.last().unwrap().kind, ManeuverKind::Arrive);
            assert_eq!(
                maneuvers.last().unwrap().point_index,
                route.points.len() - 1
            );
            assert!(
                maneuvers
                    .windows(2)
                    .all(|w| w[1].point_index >= w[0].point_index),
                "{mode:?}"
            );
            for m in maneuvers {
                // Its distance is the line's at its point, and its point is where OSRM said.
                assert_eq!(m.dist_m, route.cum_dist_m[m.point_index]);
                assert!(
                    haversine_m(m.at, route.points[m.point_index]) < 25.0,
                    "{mode:?} {m:?}"
                );
            }
        }
    }

    #[test]
    fn steps_read_like_directions() {
        let directions = parse(Mode::Car, CAR).unwrap();
        let steps = &directions.steps;
        assert_eq!(steps.len(), 10);
        assert_eq!(steps[0].arrow, Arrow::Depart);
        assert!(
            steps[0].text.starts_with("Head southwest on "),
            "{}",
            steps[0].text
        );
        assert!(steps[0].distance_m > 0.0);
        let last = steps.last().unwrap();
        assert_eq!(last.arrow, Arrow::Arrive);
        assert!(
            last.text.starts_with("Arrive at your destination"),
            "{}",
            last.text
        );
        // Every turn the session announces points at its own line.
        assert_eq!(
            directions.maneuver_steps.len(),
            directions.route.maneuvers.len()
        );
        for (m, &step) in directions
            .route
            .maneuvers
            .iter()
            .zip(&directions.maneuver_steps)
        {
            assert_eq!(Arrow::of(m.kind), steps[step].arrow, "{m:?}");
        }
    }

    #[test]
    fn indices_run_forward_whatever_the_steps_say() {
        // Two legs' worth of steps over the documented sample's three
        // points: a depart and an arrive in the middle, and a step that is
        // no turn between two that are.
        let reply = r#"{"code":"Ok","routes":[{"geometry":"_p~iF~ps|U_ulLnnqC_mqNvxq`@","duration":100,"legs":[{"steps":[
            {"name":"A","distance":10,"maneuver":{"type":"depart","location":[-120.2,38.5]}},
            {"name":"B","distance":10,"maneuver":{"type":"new name","modifier":"straight","location":[-120.95,40.7]}},
            {"name":"B","distance":0,"maneuver":{"type":"arrive","location":[-120.95,40.7]}},
            {"name":"B","distance":10,"maneuver":{"type":"depart","location":[-120.95,40.7]}},
            {"name":"C","distance":10,"maneuver":{"type":"turn","modifier":"left","location":[-126.453,43.252]}},
            {"name":"C","distance":0,"maneuver":{"type":"arrive","location":[-126.453,43.252]}}
        ]}]}]}"#;
        let route = parse(Mode::Car, reply).unwrap().route;
        let indices: Vec<usize> = route.maneuvers.iter().map(|m| m.point_index).collect();
        assert_eq!(indices, vec![0, 1, 1, 2, 2], "{:?}", route.maneuvers);
        assert!(route
            .maneuvers
            .windows(2)
            .all(|w| w[1].dist_m >= w[0].dist_m));
    }

    #[test]
    fn the_maneuver_table() {
        use ManeuverKind::*;
        let cases = [
            ("turn", "left", 0, Some(TurnLeft)),
            ("turn", "slight right", 0, Some(TurnSlightRight)),
            ("turn", "sharp left", 0, Some(TurnSharpLeft)),
            ("turn", "uturn", 0, Some(UTurn)),
            ("turn", "straight", 0, None),
            ("end of road", "right", 0, Some(TurnRight)),
            ("fork", "slight left", 0, Some(TurnSlightLeft)),
            ("on ramp", "right", 0, Some(TurnRight)),
            ("off ramp", "slight right", 0, Some(TurnSlightRight)),
            ("merge", "slight left", 0, Some(TurnSlightLeft)),
            ("roundabout", "right", 2, Some(RoundaboutExit(2))),
            ("rotary", "", 0, Some(RoundaboutExit(1))),
            ("roundabout turn", "left", 0, Some(TurnLeft)),
            ("depart", "", 0, Some(Depart)),
            ("arrive", "right", 0, Some(Arrive)),
            // A road that bends left under a new name is a turn; one that
            // goes straight on under one is not.
            ("new name", "left", 0, Some(TurnLeft)),
            ("new name", "straight", 0, None),
            ("continue", "straight", 0, None),
            ("continue", "uturn", 0, Some(UTurn)),
            ("notification", "left", 0, None),
            ("use lane", "straight", 0, None),
            ("exit roundabout", "right", 0, None),
        ];
        for (kind, modifier, exit, expected) in cases {
            assert_eq!(
                maneuver_kind(kind, modifier, exit),
                expected,
                "{kind} / {modifier}"
            );
        }
    }

    #[test]
    fn no_route_is_its_own_answer_and_garbage_is_malformed() {
        assert_eq!(parse(Mode::Car, NO_ROUTE).unwrap_err(), RouteError::NoRoute);
        assert!(matches!(
            parse(Mode::Car, "<html>504</html>"),
            Err(RouteError::Malformed(_))
        ));
        assert!(matches!(
            parse(Mode::Car, r#"{"code":"Ok","routes":[]}"#),
            Err(RouteError::Malformed(_))
        ));
        // A line of one point is no route to follow.
        let stub = r#"{"code":"Ok","routes":[{"geometry":"_p~iF~ps|U","distance":0,"duration":0,"legs":[{"steps":[]}]}]}"#;
        assert!(matches!(
            parse(Mode::Car, stub),
            Err(RouteError::Malformed(_))
        ));
        assert!(matches!(
            parse(Mode::Car, r#"{"code":"InvalidQuery","message":"bad"}"#),
            Err(RouteError::Malformed(_))
        ));
    }

    #[test]
    fn a_url_per_mode() {
        let from = LonLat::new(-121.8863, 37.3382);
        let to = LonLat::new(-121.9552, 37.3541);
        assert_eq!(
            route_url(Mode::Walk, from, to),
            "https://routing.openstreetmap.de/routed-foot/route/v1/driving/-121.886300,37.338200;-121.955200,37.354100?overview=full&steps=true&geometries=polyline"
        );
        assert!(route_url(Mode::Car, from, to).contains("/routed-car/"));
        assert!(route_url(Mode::Bike, from, to).contains("/routed-bike/"));
    }
}
