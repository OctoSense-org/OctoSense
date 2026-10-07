//! Maps' place logic: the pure functions in `apps/maps/bundle/main.splash`
//! (everything between the shared interface and `start_timeout(`), run in a
//! script VM.
use makepad_widgets::*;

const MAPS: &str = include_str!("../../../apps/maps/bundle/main.splash");

/// Evaluate `expression` after Maps' functions; it must end in `.to_json()`.
fn maps_model(expression: &str) -> serde_json::Value {
    let source = MAPS.split_once("// END shared app interface\n").unwrap().1
        .split_once("\nstart_timeout(").unwrap().0;
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm {
        host: &mut host,
        bx: Box::new(ScriptVmBase::new()),
    };
    vm.bx.captured_errors = Some(Vec::new());
    let result = vm.with_instruction_limit(500_000, |vm| {
        vm.eval(ScriptMod {
            file: "maps_model_test.splash".into(),
            code: format!("use mod.math.*\n{source}\n{expression}\n;"),
            ..Default::default()
        })
    });
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    let json = vm
        .bx
        .heap
        .string_with(result, |_, value| value.to_string())
        .unwrap();
    serde_json::from_str(&json).unwrap()
}

/// Two hits for one way, an address, and a feature without coordinates. The
/// whole body goes into a single-quoted script string, so no `'` in it.
const PHOTON: &str = concat!(
    r#"{"type":"FeatureCollection","features":["#,
    r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[-121.9486002,37.3209796]},"properties":{"osm_type":"W","osm_id":25904339,"osm_key":"place","osm_value":"neighbourhood","name":"Santana Row","city":"San Jose","state":"California","country":"United States"}},"#,
    r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[-121.9486002,37.3209796]},"properties":{"osm_type":"W","osm_id":25904339,"osm_key":"place","osm_value":"neighbourhood","name":"Santana Row"}},"#,
    r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[-121.885,37.335]},"properties":{"osm_type":"N","osm_id":10735327671,"osm_key":"amenity","osm_value":"fast_food","housenumber":"10","street":"Market Street","city":"San Jose"}},"#,
    r#"{"type":"Feature","properties":{"name":"No geometry"}}"#,
    r#"]}"#
);

#[test]
fn maps_reads_photon_results_as_places() {
    let hits = maps_model(&format!("photon_hits('{PHOTON}').to_json()"));
    assert_eq!(
        hits,
        serde_json::json!([
            {"id": "W:25904339", "name": "Santana Row", "cat": "Neighbourhood",
             "label": "San Jose, California, United States", "lat": 37.3209796, "lon": -121.9486002},
            {"id": "N:10735327671", "name": "10 Market Street", "cat": "Fast food",
             "label": "San Jose", "lat": 37.335, "lon": -121.885}
        ])
    );
}

#[test]
fn maps_tells_no_places_from_a_bad_answer() {
    let out = maps_model(r#"[photon_hits('{"features":[]}').len() photon_hits('<html>busy</html>') == nil].to_json()"#);
    assert_eq!(out, serde_json::json!([0, true]));
}

/// Address parts that contain one another, names the label must not repeat,
/// a street, and hits missing a name, a street, a city or an OpenStreetMap id.
const ODD_PHOTON: &str = concat!(
    r#"{"type":"FeatureCollection","features":["#,
    r#"{"geometry":{"coordinates":[-94.58,39.1]},"properties":{"osm_type":"R","osm_id":1,"osm_key":"place","osm_value":"city","name":"Kansas City","city":"Kansas City","state":"Kansas","country":"United States"}},"#,
    r#"{"geometry":{"coordinates":[-86.16,39.76]},"properties":{"osm_type":"W","osm_id":2,"osm_key":"leisure","osm_value":"stadium","name":"Lucas Oil Stadium","city":"Indianapolis","state":"Indiana","country":"United States"}},"#,
    r#"{"geometry":{"coordinates":[-73.99,40.75]},"properties":{"osm_type":"W","osm_id":3,"osm_key":"tourism","osm_value":"attraction","name":"Empire State Building","city":"New York","state":"New York","country":"United States"}},"#,
    r#"{"geometry":{"coordinates":[13.32,52.46]},"properties":{"osm_type":"N","osm_id":4,"osm_key":"amenity","osm_value":"townhall","name":"Rathaus","street":"Berliner Straße","city":"Berlin","country":"Deutschland"}},"#,
    r#"{"geometry":{"coordinates":[-121.89,37.33]},"properties":{"osm_type":"W","osm_id":5,"osm_key":"highway","osm_value":"residential","name":"Market Street","city":"San Jose"}},"#,
    r#"{"geometry":{"coordinates":[-119.79,36.74]},"properties":{"osm_type":"N","osm_key":"place","osm_value":"town","city":"Fresno","state":"California","country":"United States"}},"#,
    r#"{"geometry":{"coordinates":[-117.1,38.5]},"properties":{"osm_key":"natural","osm_value":"peak","state":"Nevada"}}"#,
    r#"]}"#
);

#[test]
fn maps_reads_odd_photon_answers() {
    let hits = maps_model(&format!("photon_hits('{ODD_PHOTON}').to_json()"));
    assert_eq!(
        hits,
        serde_json::json!([
            {"id": "R:1", "name": "Kansas City", "cat": "City",
             "label": "Kansas, United States", "lat": 39.1, "lon": -94.58},
            {"id": "W:2", "name": "Lucas Oil Stadium", "cat": "Stadium",
             "label": "Indianapolis, Indiana, United States", "lat": 39.76, "lon": -86.16},
            {"id": "W:3", "name": "Empire State Building", "cat": "Attraction",
             "label": "New York, United States", "lat": 40.75, "lon": -73.99},
            {"id": "N:4", "name": "Rathaus", "cat": "Town hall",
             "label": "Berliner Straße, Berlin, Deutschland", "lat": 52.46, "lon": 13.32},
            {"id": "W:5", "name": "Market Street", "cat": "Street",
             "label": "San Jose", "lat": 37.33, "lon": -121.89},
            {"id": "", "name": "Fresno", "cat": "Town",
             "label": "California, United States", "lat": 36.74, "lon": -119.79},
            {"id": "", "name": "Unnamed place", "cat": "Peak",
             "label": "Nevada", "lat": 38.5, "lon": -117.1}
        ])
    );
    // Photon's own error message and a malformed answer are not "no places";
    // `maps_model` also fails on any script error they raise.
    let out = maps_model(
        r#"[photon_hits('{"message":"bad request"}') == nil photon_hits('{"features":"oops"}') == nil].to_json()"#,
    );
    assert_eq!(out, serde_json::json!([true, true]));
}

#[test]
fn maps_names_categories_like_a_person_would() {
    let out = maps_model(
        r#"[category("amenity", "fuel") category("building", "yes") category("shop", "ice_cream") category("amenity", "")
            category("amenity", "place_of_worship") category("building", "house") category("highway", "residential")
            category("highway", "bus_stop") category("highway", "primary_link")].to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([
            "Gas station", "Building", "Ice cream", "Amenity", "Place of worship", "Address", "Street",
            "Bus stop", "Street"
        ])
    );
}
