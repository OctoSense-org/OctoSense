//! Distances and bearings (the framework's), the polyline decoder, the
//! camera that fits a box into the part of the viewport the panels leave
//! free, and distance and duration text in metric and imperial.

pub use makepad_map_nav::geo::{
    bearing_deg, cumulative_distances, haversine_m, lon_lat_to_norm, norm_to_lon_lat, LonLat,
};

/// `MapView`'s world is this many pixels across at zoom 0 and doubles per
/// zoom level (`widgets/src/map/geometry.rs`, `TILE_SIZE`).
const WORLD_PX_AT_ZOOM_0: f64 = 256.0;

/// Decode a polyline5 string (Google's encoding, OSRM's `geometries=polyline`)
/// into points. The encoding is latitude first; the points are not.
pub fn decode_polyline5(encoded: &str) -> Vec<LonLat> {
    let mut bytes = encoded.bytes();
    let mut points = Vec::with_capacity(encoded.len() / 4);
    let (mut lat, mut lon) = (0i64, 0i64);
    // A torn tail ends the line at the last whole point.
    while let (Some(dlat), Some(dlon)) = (next_delta(&mut bytes), next_delta(&mut bytes)) {
        // A sum that leaves the type ends the line, as a torn tail does.
        let (Some(next_lat), Some(next_lon)) = (lat.checked_add(dlat), lon.checked_add(dlon))
        else {
            break;
        };
        (lat, lon) = (next_lat, next_lon);
        points.push(LonLat::new(lon as f64 * 1e-5, lat as f64 * 1e-5));
    }
    points
}

/// One zigzag varint of the encoding: five bits a byte, low group first,
/// bit 5 set on all but the last. `None` when the bytes run out first.
fn next_delta(bytes: &mut impl Iterator<Item = u8>) -> Option<i64> {
    let (mut value, mut shift) = (0i64, 0u32);
    loop {
        let group = i64::from(bytes.next()?.checked_sub(63)?);
        value |= (group & 0x1f) << shift;
        shift += 5;
        if group < 0x20 {
            break;
        }
        // The whole globe is 26 bits of fifth decimals; nothing the
        // services send is longer than this, and a hostile string is.
        if shift > 35 {
            return None;
        }
    }
    Some(if value & 1 == 1 {
        !(value >> 1)
    } else {
        value >> 1
    })
}

/// A box in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl Bounds {
    /// The box around `points`; `None` for none.
    pub fn of(points: &[LonLat]) -> Option<Bounds> {
        let first = points.first()?;
        let start = Bounds {
            west: first.lon,
            south: first.lat,
            east: first.lon,
            north: first.lat,
        };
        Some(points.iter().fold(start, |b, p| Bounds {
            west: b.west.min(p.lon),
            south: b.south.min(p.lat),
            east: b.east.max(p.lon),
            north: b.north.max(p.lat),
        }))
    }

    pub fn center(&self) -> LonLat {
        LonLat::new(
            (self.west + self.east) * 0.5,
            (self.south + self.north) * 0.5,
        )
    }
}

/// The parts of the viewport something else covers, in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub center: LonLat,
    pub zoom: f64,
}

/// The flat, north-up camera that shows `bounds` in the room `insets` leave
/// free in a `viewport` of (width, height) pixels, zoomed in no further than
/// `max_zoom`. The centre is shifted so the box sits in the middle of the
/// free room rather than the middle of the viewport.
pub fn fit_camera(bounds: &Bounds, viewport: (f64, f64), insets: &Insets, max_zoom: f64) -> Camera {
    // Mercator, where the map is linear: north has the smaller y.
    let (west, north) = lon_lat_to_norm(LonLat::new(bounds.west, bounds.north));
    let (east, south) = lon_lat_to_norm(LonLat::new(bounds.east, bounds.south));
    // Panels that cover everything still leave a pixel, so the zoom stays a number.
    let free_w = (viewport.0 - insets.left - insets.right).max(1.0);
    let free_h = (viewport.1 - insets.top - insets.bottom).max(1.0);
    // A span of nothing (one point) fits at any zoom: the cap decides.
    let zoom_for = |free: f64, span: f64| {
        if span > 0.0 {
            (free / (WORLD_PX_AT_ZOOM_0 * span)).log2()
        } else {
            f64::INFINITY
        }
    };
    let zoom = zoom_for(free_w, east - west)
        .min(zoom_for(free_h, south - north))
        .clamp(0.0, max_zoom);
    // The free room's middle is off the viewport's by half the difference
    // of the opposite insets; the camera moves the other way by as much.
    let world = WORLD_PX_AT_ZOOM_0 * 2f64.powf(zoom);
    let shift_x = (insets.left - insets.right) * 0.5 / world;
    let shift_y = (insets.top - insets.bottom) * 0.5 / world;
    Camera {
        center: norm_to_lon_lat(
            (west + east) * 0.5 - shift_x,
            (north + south) * 0.5 - shift_y,
        ),
        zoom,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Units {
    #[default]
    Metric,
    Imperial,
}

impl Units {
    /// What road signs use in the country with this ISO 3166 alpha-2 code.
    pub fn for_country(code: &str) -> Units {
        match code.to_ascii_uppercase().as_str() {
            "US" | "GB" | "LR" | "MM" => Units::Imperial,
            _ => Units::Metric,
        }
    }
}

/// `350 m`, `2.4 km`, `12 km`; `500 ft`, `0.3 mi`, `12 mi`.
pub fn distance_text(meters: f64, units: Units) -> String {
    let meters = meters.max(0.0);
    match units {
        Units::Metric => {
            let rounded = round_to(meters, if meters < 100.0 { 5.0 } else { 10.0 });
            if rounded < 1_000.0 {
                format!("{rounded:.0} m")
            } else {
                large_text(meters / 1_000.0, "km")
            }
        }
        Units::Imperial => {
            let miles = meters / METERS_PER_MILE;
            if miles < 0.1 {
                let feet = meters * FEET_PER_METER;
                format!(
                    "{:.0} ft",
                    round_to(feet, if feet < 100.0 { 10.0 } else { 50.0 })
                )
            } else {
                large_text(miles, "mi")
            }
        }
    }
}

const FEET_PER_METER: f64 = 3.280_84;
const METERS_PER_MILE: f64 = 1_609.344;

fn round_to(value: f64, step: f64) -> f64 {
    (value / step).round() * step
}

/// Kilometres or miles: a decimal under ten, whole numbers from there.
fn large_text(value: f64, unit: &str) -> String {
    // Rounded first, so 9.96 reads `10`, not `10.0`.
    let tenths = (value * 10.0).round() / 10.0;
    if tenths < 10.0 {
        format!("{tenths:.1} {unit}")
    } else {
        format!("{value:.0} {unit}")
    }
}

/// `1 min`, `8 min`, `1 hr`, `1 hr 44 min`.
pub fn duration_text(seconds: f64) -> String {
    let minutes = whole_minutes(seconds).max(1);
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} hr"),
        (h, m) => format!("{h} hr {m} min"),
    }
}

fn whole_minutes(seconds: f64) -> u32 {
    (seconds.max(0.0) / 60.0).round() as u32
}

/// The clock time `seconds` after `now_minutes_of_day`, as `3:42 PM`.
pub fn arrival_text(now_minutes_of_day: u32, seconds: f64) -> String {
    const DAY: u32 = 24 * 60;
    let at = (now_minutes_of_day % DAY + whole_minutes(seconds) % DAY) % DAY;
    let (hour, minute) = (at / 60, at % 60);
    let half = if hour < 12 { "AM" } else { "PM" };
    let hour12 = if hour % 12 == 0 { 12 } else { hour % 12 };
    format!("{hour12}:{minute:02} {half}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAN_JOSE: LonLat = LonLat {
        lon: -121.8863,
        lat: 37.3382,
    };
    const SAN_FRANCISCO: LonLat = LonLat {
        lon: -122.4194,
        lat: 37.7749,
    };

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    /// Where `point` lands in a viewport showing `camera`, in pixels from the
    /// viewport's top left: the projection `fit_camera` has to agree with.
    fn project(camera: &Camera, viewport: (f64, f64), point: LonLat) -> (f64, f64) {
        let world = WORLD_PX_AT_ZOOM_0 * 2f64.powf(camera.zoom);
        let (cx, cy) = lon_lat_to_norm(camera.center);
        let (px, py) = lon_lat_to_norm(point);
        (
            (px - cx) * world + viewport.0 * 0.5,
            (py - cy) * world + viewport.1 * 0.5,
        )
    }

    #[test]
    fn the_frameworks_distance_is_the_great_circle() {
        // 67.6 km between the two city centres.
        assert!(close(haversine_m(SAN_JOSE, SAN_FRANCISCO), 67_600.0, 700.0));
    }

    #[test]
    fn decodes_the_documented_sample() {
        // The example in Google's description of the format.
        let points = decode_polyline5("_p~iF~ps|U_ulLnnqC_mqNvxq`@");
        assert_eq!(points.len(), 3);
        let expected = [(-120.2, 38.5), (-120.95, 40.7), (-126.453, 43.252)];
        for (point, (lon, lat)) in points.iter().zip(expected) {
            assert!(
                close(point.lon, lon, 1e-5) && close(point.lat, lat, 1e-5),
                "{point:?}"
            );
        }
    }

    #[test]
    fn decodes_nothing_from_nothing_and_stops_at_a_torn_tail() {
        assert!(decode_polyline5("").is_empty());
        // The sample cut in the middle of its second point: the first stands.
        assert_eq!(decode_polyline5("_p~iF~ps|U_ulL").len(), 1);
    }

    #[test]
    fn a_hostile_line_ends_where_it_stops_making_sense() {
        // A varint that never ends, and deltas as large as the bound allows,
        // over and over: no panic, and nothing after the nonsense.
        assert!(decode_polyline5(&"~".repeat(64)).is_empty());
        let huge = "~~~~~~]".repeat(200_000);
        let points = decode_polyline5(&huge);
        assert!(points.len() <= 100_000);
    }

    #[test]
    fn bounds_wrap_their_points() {
        assert_eq!(Bounds::of(&[]), None);
        let bounds = Bounds::of(&[SAN_JOSE, SAN_FRANCISCO]).unwrap();
        assert_eq!(
            bounds,
            Bounds {
                west: -122.4194,
                south: 37.3382,
                east: -121.8863,
                north: 37.7749
            }
        );
        let center = bounds.center();
        assert!(close(center.lon, -122.15285, 1e-9) && close(center.lat, 37.55655, 1e-9));
    }

    #[test]
    fn a_fitted_box_fills_the_free_room_and_no_more() {
        let viewport = (400.0, 800.0);
        let insets = Insets {
            top: 150.0,
            bottom: 250.0,
            left: 24.0,
            right: 24.0,
        };
        let bounds = Bounds::of(&[SAN_JOSE, SAN_FRANCISCO]).unwrap();
        let camera = fit_camera(&bounds, viewport, &insets, 17.0);
        let (left, top) = project(
            &camera,
            viewport,
            LonLat {
                lon: bounds.west,
                lat: bounds.north,
            },
        );
        let (right, bottom) = project(
            &camera,
            viewport,
            LonLat {
                lon: bounds.east,
                lat: bounds.south,
            },
        );
        // Inside the free room on every side.
        assert!(
            left >= insets.left - 0.5 && right <= viewport.0 - insets.right + 0.5,
            "{left} {right}"
        );
        assert!(
            top >= insets.top - 0.5 && bottom <= viewport.1 - insets.bottom + 0.5,
            "{top} {bottom}"
        );
        // And touching it on the tighter axis.
        let fills_x = close(right - left, viewport.0 - insets.left - insets.right, 1.0);
        let fills_y = close(bottom - top, viewport.1 - insets.top - insets.bottom, 1.0);
        assert!(fills_x || fills_y, "{} x {}", right - left, bottom - top);
        // Centred in the free room, not in the viewport.
        assert!(close(
            (left + right) * 0.5,
            (insets.left + viewport.0 - insets.right) * 0.5,
            1.0
        ));
        assert!(close(
            (top + bottom) * 0.5,
            (insets.top + viewport.1 - insets.bottom) * 0.5,
            1.0
        ));
    }

    #[test]
    fn a_wide_box_is_bound_by_its_width_and_a_tall_one_by_its_height() {
        let viewport = (400.0, 400.0);
        let none = Insets::default();
        let wide = Bounds {
            west: -122.0,
            south: 37.0,
            east: -121.0,
            north: 37.1,
        };
        let tall = Bounds {
            west: -122.0,
            south: 37.0,
            east: -121.9,
            north: 38.0,
        };
        let wide_cam = fit_camera(&wide, viewport, &none, 20.0);
        let tall_cam = fit_camera(&tall, viewport, &none, 20.0);
        let span = |cam: &Camera, b: &Bounds| {
            let (l, t) = project(
                cam,
                viewport,
                LonLat {
                    lon: b.west,
                    lat: b.north,
                },
            );
            let (r, bt) = project(
                cam,
                viewport,
                LonLat {
                    lon: b.east,
                    lat: b.south,
                },
            );
            (r - l, bt - t)
        };
        let (w, h) = span(&wide_cam, &wide);
        assert!(close(w, 400.0, 1.0) && h < 400.0);
        let (w, h) = span(&tall_cam, &tall);
        assert!(close(h, 400.0, 1.0) && w < 400.0);
    }

    #[test]
    fn a_single_point_and_a_room_of_nothing_both_stop_at_the_zoom_cap() {
        let point = Bounds::of(&[SAN_JOSE]).unwrap();
        let camera = fit_camera(&point, (400.0, 800.0), &Insets::default(), 16.0);
        assert_eq!(camera.zoom, 16.0);
        assert!(
            close(camera.center.lon, SAN_JOSE.lon, 1e-9)
                && close(camera.center.lat, SAN_JOSE.lat, 1e-9)
        );
        // Panels that leave no room cannot drive the zoom to nonsense.
        let covered = Insets {
            top: 500.0,
            bottom: 500.0,
            ..Insets::default()
        };
        let camera = fit_camera(
            &Bounds::of(&[SAN_JOSE, SAN_FRANCISCO]).unwrap(),
            (400.0, 800.0),
            &covered,
            16.0,
        );
        assert!(camera.zoom.is_finite() && camera.zoom <= 16.0 && camera.zoom >= 0.0);
    }

    #[test]
    fn units_follow_the_road_signs() {
        for code in ["US", "us", "GB", "LR", "MM"] {
            assert_eq!(Units::for_country(code), Units::Imperial, "{code}");
        }
        for code in ["NL", "DE", "CA", "", "USA"] {
            assert_eq!(Units::for_country(code), Units::Metric, "{code}");
        }
    }

    #[test]
    fn metric_distances() {
        assert_eq!(distance_text(0.0, Units::Metric), "0 m");
        assert_eq!(distance_text(14.0, Units::Metric), "15 m");
        assert_eq!(distance_text(348.0, Units::Metric), "350 m");
        assert_eq!(distance_text(999.0, Units::Metric), "1.0 km");
        assert_eq!(distance_text(2_440.0, Units::Metric), "2.4 km");
        assert_eq!(distance_text(12_400.0, Units::Metric), "12 km");
    }

    #[test]
    fn imperial_distances() {
        assert_eq!(distance_text(30.0, Units::Imperial), "100 ft");
        assert_eq!(distance_text(150.0, Units::Imperial), "500 ft");
        // A tenth of a mile is where feet stop.
        assert_eq!(distance_text(170.0, Units::Imperial), "0.1 mi");
        assert_eq!(distance_text(2_092.0, Units::Imperial), "1.3 mi");
        assert_eq!(distance_text(19_956.0, Units::Imperial), "12 mi");
    }

    #[test]
    fn durations() {
        assert_eq!(duration_text(0.0), "1 min");
        assert_eq!(duration_text(29.0), "1 min");
        assert_eq!(duration_text(629.0), "10 min");
        assert_eq!(duration_text(3_600.0), "1 hr");
        assert_eq!(duration_text(6_234.0), "1 hr 44 min");
    }

    #[test]
    fn arrival_times() {
        assert_eq!(arrival_text(15 * 60 + 32, 600.0), "3:42 PM");
        assert_eq!(arrival_text(0, 0.0), "12:00 AM");
        assert_eq!(arrival_text(11 * 60 + 59, 60.0), "12:00 PM");
        // Past midnight wraps.
        assert_eq!(arrival_text(23 * 60 + 50, 1_200.0), "12:10 AM");
    }
}
