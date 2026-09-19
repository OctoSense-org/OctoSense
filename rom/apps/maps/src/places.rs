//! Places from Photon (photon.komoot.io, OpenStreetMap data, no key): the
//! search and reverse URLs, and their GeoJSON replies as `Place` rows with
//! the lines the interface shows.

use crate::geo::LonLat;
use makepad_widgets::makepad_micro_serde::*;

const SEARCH: &str = "https://photon.komoot.io/api/";
const REVERSE: &str = "https://photon.komoot.io/reverse";
/// A page of results: what fits a phone without scrolling far.
const SEARCH_LIMIT: usize = 8;

/// What a place is, as far as its row's icon goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaceKind {
    #[default]
    Place,
    Food,
    Shop,
    Transit,
    Lodging,
    Street,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place {
    pub name: String,
    /// `University`, `Street`, `Marketing office`; empty when unknown.
    pub category: String,
    pub kind: PlaceKind,
    /// `500 El Camino Real, Santa Clara, California`; empty when unknown.
    pub address: String,
    pub pos: LonLat,
    /// ISO 3166 alpha-2, upper case; empty when unknown.
    pub country_code: String,
    /// Label and value rows for the sheet: a phone number, opening hours.
    pub details: Vec<(String, String)>,
}

/// Places matching `query`, the ones near `near` first.
pub fn search_url(query: &str, near: LonLat) -> String {
    format!(
        "{SEARCH}?q={}&limit={SEARCH_LIMIT}&lang=en&lat={:.4}&lon={:.4}",
        percent_encode(query),
        near.lat,
        near.lon
    )
}

/// Everything but RFC 3986's unreserved characters, byte by byte.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// What is at `at`.
pub fn reverse_url(at: LonLat) -> String {
    format!("{REVERSE}?lat={:.6}&lon={:.6}&lang=en", at.lat, at.lon)
}

/// A reply of either endpoint, in the service's order.
pub fn parse(json: &str) -> Result<Vec<Place>, String> {
    // Lenient: Photon sends fields we do not model (ids, extents, postcodes).
    let reply: Reply = DeJson::deserialize_json_lenient(json).map_err(|e| format!("{e:?}"))?;
    let features = reply.features.ok_or("not a Photon reply: no features")?;
    Ok(features.into_iter().filter_map(place).collect())
}

#[derive(Clone, Debug, Default, DeJson)]
struct Reply {
    features: Option<Vec<Feature>>,
}

#[derive(Clone, Debug, Default, DeJson)]
struct Feature {
    geometry: Option<Geometry>,
    properties: Option<Properties>,
}

#[derive(Clone, Debug, Default, DeJson)]
struct Geometry {
    coordinates: Option<Vec<f64>>,
}

#[derive(Clone, Debug, Default, DeJson)]
struct Properties {
    name: Option<String>,
    housenumber: Option<String>,
    street: Option<String>,
    city: Option<String>,
    state: Option<String>,
    countrycode: Option<String>,
    osm_key: Option<String>,
    osm_value: Option<String>,
}

/// One feature as a row; `None` when it has no point to show it at, or
/// nothing at all to call it.
fn place(feature: Feature) -> Option<Place> {
    let pos = match feature.geometry?.coordinates?.as_slice() {
        [lon, lat, ..] if lon.is_finite() && lat.is_finite() => LonLat::new(*lon, *lat),
        _ => return None,
    };
    let p = feature.properties.unwrap_or_default();
    let street_line = match (filled(&p.housenumber), filled(&p.street)) {
        (Some(number), Some(street)) => Some(format!("{number} {street}")),
        (None, Some(street)) => Some(street.to_string()),
        (_, None) => None,
    };
    let name = filled(&p.name)
        .map(str::to_string)
        .or_else(|| street_line.clone())?;
    // A part that only repeats the name says nothing.
    let address = [street_line.as_deref(), filled(&p.city), filled(&p.state)]
        .into_iter()
        .flatten()
        .filter(|part| *part != name)
        .collect::<Vec<_>>()
        .join(", ");
    let (key, value) = (
        filled(&p.osm_key).unwrap_or(""),
        filled(&p.osm_value).unwrap_or(""),
    );
    Some(Place {
        name,
        category: category(key, value),
        kind: kind(key, value),
        address,
        pos,
        country_code: filled(&p.countrycode).unwrap_or("").to_ascii_uppercase(),
        details: Vec::new(),
    })
}

/// The text of a field that has some.
fn filled(field: &Option<String>) -> Option<&str> {
    field
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn category(key: &str, value: &str) -> String {
    match (key, value) {
        ("", _) => String::new(),
        ("highway", "bus_stop") => "Bus stop".into(),
        ("highway", _) => "Street".into(),
        ("office", "yes" | "") => "Office".into(),
        ("office", kind) => format!("{} office", humanise(kind)),
        // `building=yes`: the key is all that is known.
        (_, "yes" | "") => humanise(key),
        (_, kind) => humanise(kind),
    }
}

fn kind(key: &str, value: &str) -> PlaceKind {
    match (key, value) {
        (
            "amenity",
            "restaurant" | "cafe" | "fast_food" | "bar" | "pub" | "food_court" | "ice_cream"
            | "biergarten",
        ) => PlaceKind::Food,
        ("shop", _) => PlaceKind::Shop,
        ("railway" | "public_transport" | "aeroway", _)
        | ("highway", "bus_stop")
        | ("amenity", "bus_station" | "ferry_terminal") => PlaceKind::Transit,
        ("tourism", "hotel" | "motel" | "hostel" | "guest_house" | "apartment" | "camp_site") => {
            PlaceKind::Lodging
        }
        ("highway", _) => PlaceKind::Street,
        _ => PlaceKind::Place,
    }
}

/// `fast_food` as `Fast food`.
fn humanise(tag: &str) -> String {
    let spaced = tag.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A long press, before (or without) the reverse lookup's answer.
pub fn dropped_pin(at: LonLat) -> Place {
    Place {
        name: "Dropped pin".into(),
        address: coordinates_text(at),
        pos: at,
        ..Place::default()
    }
}

/// A tapped map pin (`MapViewAction::PinTapped`) with the attributes the
/// layer knows: `name` titles it, the rest are the sheet's detail rows.
pub fn from_pin_info(lon: f64, lat: f64, info: &[(String, String)]) -> Place {
    let text = |value: &String| {
        Some(value.trim())
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let name = info
        .iter()
        .find(|(key, _)| key == "name")
        .and_then(|(_, value)| text(value));
    let details = info
        .iter()
        .filter(|(key, _)| key != "name")
        .filter_map(|(key, value)| Some((humanise(key), text(value)?)))
        .collect();
    Place {
        name: name.unwrap_or_else(|| "Pin".into()),
        pos: LonLat::new(lon, lat),
        details,
        ..Place::default()
    }
}

/// `37.33820, -121.88630`: latitude first, as people read coordinates.
pub fn coordinates_text(at: LonLat) -> String {
    format!("{:.5}, {:.5}", at.lat, at.lon)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH_REPLY: &str = include_str!("../tests/fixtures/photon-search.json");
    const REVERSE_REPLY: &str = include_str!("../tests/fixtures/photon-reverse.json");
    const EMPTY_REPLY: &str = include_str!("../tests/fixtures/photon-empty.json");

    #[test]
    fn a_university_with_its_address() {
        let places = parse(SEARCH_REPLY).unwrap();
        assert_eq!(places.len(), 3);
        let place = &places[0];
        assert_eq!(place.name, "Santa Clara University");
        assert_eq!(place.category, "University");
        assert_eq!(place.kind, PlaceKind::Place);
        assert_eq!(place.address, "500 El Camino Real, Santa Clara, California");
        assert_eq!(place.country_code, "US");
        assert!(
            (place.pos.lon - -121.936544).abs() < 1e-9 && (place.pos.lat - 37.3486243).abs() < 1e-9
        );
        // An office reads as one.
        assert_eq!(places[2].category, "Marketing office");
    }

    #[test]
    fn a_street_is_a_street_and_does_not_repeat_itself() {
        let places = parse(REVERSE_REPLY).unwrap();
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].name, "East Santa Clara Street");
        assert_eq!(places[0].category, "Street");
        assert_eq!(places[0].kind, PlaceKind::Street);
        // The street is the name, so the address starts at the city.
        assert_eq!(places[0].address, "San Jose, California");
    }

    #[test]
    fn nothing_found_is_not_an_error_and_a_torn_body_is() {
        assert_eq!(parse(EMPTY_REPLY), Ok(vec![]));
        assert!(parse(&SEARCH_REPLY[..SEARCH_REPLY.len() / 2]).is_err());
        assert!(parse("<html>502 Bad Gateway</html>").is_err());
    }

    #[test]
    fn a_feature_with_no_name_takes_its_address_and_one_with_no_point_is_dropped() {
        let unnamed = r#"{"features":[
            {"geometry":{"type":"Point","coordinates":[-121.9,37.3]},
             "properties":{"osm_key":"building","osm_value":"yes","housenumber":"12","street":"Oak Street","city":"San Jose","countrycode":"us"}},
            {"geometry":{"type":"Point","coordinates":[-121.9]},"properties":{"name":"Half a point"}},
            {"properties":{"name":"No geometry"}}
        ]}"#;
        let places = parse(unnamed).unwrap();
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].name, "12 Oak Street");
        assert_eq!(places[0].category, "Building");
        assert_eq!(places[0].address, "San Jose");
        assert_eq!(places[0].country_code, "US");
    }

    #[test]
    fn kinds_pick_the_icon() {
        let reply = |key: &str, value: &str| {
            format!(
                r#"{{"features":[{{"geometry":{{"coordinates":[1.0,2.0]}},"properties":{{"name":"X","osm_key":"{key}","osm_value":"{value}"}}}}]}}"#
            )
        };
        let kind = |key: &str, value: &str| parse(&reply(key, value)).unwrap()[0].kind;
        assert_eq!(kind("amenity", "restaurant"), PlaceKind::Food);
        assert_eq!(kind("amenity", "cafe"), PlaceKind::Food);
        assert_eq!(kind("shop", "supermarket"), PlaceKind::Shop);
        assert_eq!(kind("railway", "station"), PlaceKind::Transit);
        assert_eq!(kind("highway", "bus_stop"), PlaceKind::Transit);
        assert_eq!(kind("tourism", "hotel"), PlaceKind::Lodging);
        assert_eq!(kind("highway", "residential"), PlaceKind::Street);
        assert_eq!(kind("place", "city"), PlaceKind::Place);
        // Underscores are spaces in a category.
        assert_eq!(
            parse(&reply("amenity", "fast_food")).unwrap()[0].category,
            "Fast food"
        );
    }

    #[test]
    fn urls_escape_what_the_person_typed() {
        let url = search_url("café & bar", LonLat::new(-121.95, 37.35));
        assert_eq!(url, "https://photon.komoot.io/api/?q=caf%C3%A9%20%26%20bar&limit=8&lang=en&lat=37.3500&lon=-121.9500");
        assert_eq!(
            reverse_url(LonLat::new(-121.8863, 37.3382)),
            "https://photon.komoot.io/reverse?lat=37.338200&lon=-121.886300&lang=en"
        );
    }

    #[test]
    fn a_dropped_pin_is_named_by_where_it_is() {
        let pin = dropped_pin(LonLat::new(-121.8863, 37.3382));
        assert_eq!(pin.name, "Dropped pin");
        assert_eq!(pin.address, "37.33820, -121.88630");
        assert_eq!(pin.kind, PlaceKind::Place);
    }

    #[test]
    fn a_tapped_pin_keeps_what_the_layer_knows() {
        let info = vec![
            ("name".to_string(), "Fastned".to_string()),
            ("opening_hours".to_string(), "24/7".to_string()),
            ("phone".to_string(), "+31 20 123".to_string()),
            ("empty".to_string(), "  ".to_string()),
        ];
        let place = from_pin_info(4.9, 52.37, &info);
        assert_eq!(place.name, "Fastned");
        assert_eq!(
            place.details,
            vec![
                ("Opening hours".to_string(), "24/7".to_string()),
                ("Phone".to_string(), "+31 20 123".to_string())
            ]
        );
        // No name: the pin still opens.
        assert_eq!(from_pin_info(4.9, 52.37, &[]).name, "Pin");
    }
}
