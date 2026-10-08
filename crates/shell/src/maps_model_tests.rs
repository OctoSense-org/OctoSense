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
/// Then features whose coordinates aren't a place: text (numbers as text
/// too), null, 0,0, a latitude of 95 or a longitude of 181, bare `NaN` and
/// `inf`, and a single number.
const ODD_PHOTON: &str = concat!(
    r#"{"type":"FeatureCollection","features":["#,
    r#"{"geometry":{"coordinates":[-94.58,39.1]},"properties":{"osm_type":"R","osm_id":1,"osm_key":"place","osm_value":"city","name":"Kansas City","city":"Kansas City","state":"Kansas","country":"United States"}},"#,
    r#"{"geometry":{"coordinates":[-86.16,39.76]},"properties":{"osm_type":"W","osm_id":2,"osm_key":"leisure","osm_value":"stadium","name":"Lucas Oil Stadium","city":"Indianapolis","state":"Indiana","country":"United States"}},"#,
    r#"{"geometry":{"coordinates":[-73.99,40.75]},"properties":{"osm_type":"W","osm_id":3,"osm_key":"tourism","osm_value":"attraction","name":"Empire State Building","city":"New York","state":"New York","country":"United States"}},"#,
    r#"{"geometry":{"coordinates":[13.32,52.46]},"properties":{"osm_type":"N","osm_id":4,"osm_key":"amenity","osm_value":"townhall","name":"Rathaus","street":"Berliner Straße","city":"Berlin","country":"Deutschland"}},"#,
    r#"{"geometry":{"coordinates":[-121.89,37.33]},"properties":{"osm_type":"W","osm_id":5,"osm_key":"highway","osm_value":"residential","name":"Market Street","city":"San Jose"}},"#,
    r#"{"geometry":{"coordinates":[-119.79,36.74]},"properties":{"osm_type":"N","osm_key":"place","osm_value":"town","city":"Fresno","state":"California","country":"United States"}},"#,
    r#"{"geometry":{"coordinates":[-117.1,38.5]},"properties":{"osm_key":"natural","osm_value":"peak","state":"Nevada"}},"#,
    r#"{"geometry":{"coordinates":["a","b"]},"properties":{"osm_type":"N","osm_id":6,"name":"Text"}},"#,
    r#"{"geometry":{"coordinates":["-121.9","37.3"]},"properties":{"osm_type":"N","osm_id":7,"name":"Number text"}},"#,
    r#"{"geometry":{"coordinates":[null,null]},"properties":{"osm_type":"N","osm_id":8,"name":"Null"}},"#,
    r#"{"geometry":{"coordinates":[0,0]},"properties":{"osm_type":"N","osm_id":9,"name":"Zero"}},"#,
    r#"{"geometry":{"coordinates":[1,95]},"properties":{"osm_type":"N","osm_id":10,"name":"North"}},"#,
    r#"{"geometry":{"coordinates":[181,1]},"properties":{"osm_type":"N","osm_id":11,"name":"East"}},"#,
    r#"{"geometry":{"coordinates":[NaN,inf]},"properties":{"osm_type":"N","osm_id":12,"name":"Bare words"}},"#,
    r#"{"geometry":{"coordinates":[1,NaN]},"properties":{"osm_type":"N","osm_id":13,"name":"Half NaN"}},"#,
    r#"{"geometry":{"coordinates":[1]},"properties":{"osm_type":"N","osm_id":14,"name":"One"}}"#,
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
            // Nothing from the features that aren't places, and no raise
            // (`maps_model` fails on any): one would leave "Searching…" up.
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

const OVERPASS: &str = r#"{"version":0.6,"elements":[{"type":"node","id":10735327671,"tags":{"amenity":"restaurant","name":"Pizza Place","opening_hours":"Mo-Su 11:00-22:00","contact:phone":"+1 408 555 0100","website":"http://pizza.example.com;https://other.example.com","cuisine":"pizza;italian_pizza"}}]}"#;

#[test]
fn maps_reads_a_places_hours_phone_website_and_cuisine() {
    let d = maps_model(&format!("place_details('{OVERPASS}').to_json()"));
    assert_eq!(
        d,
        serde_json::json!({"hours": "Mo-Su 11:00-22:00", "phone": "+1 408 555 0100",
            "website": "https://pizza.example.com", "cuisine": "pizza, italian pizza"})
    );
    // Overpass found nothing: no details. Not Overpass's answer: nil, so the
    // caller tries again and keeps nothing.
    let none = maps_model(r#"[place_details('{"elements":[]}') place_details('<html/>')].to_json()"#);
    let blank = serde_json::json!({"hours": "", "phone": "", "website": "", "cuisine": ""});
    assert_eq!(none, serde_json::json!([blank, null]));
}

/// Overpass's own runtime error: HTTP 200, no elements, and a `remark`.
const OVERPASS_REMARK: &str = r#"{"version":0.6,"elements":[],"remark":"runtime error: Query timed out in \"query\" at line 1 after 10 seconds."}"#;

#[test]
fn maps_tells_an_overpass_error_from_no_details() {
    // A Splash string turns `\"` into `"` (single-quoted too), which would cut
    // the remark short, so the body goes in with its `\` doubled.
    let remark = OVERPASS_REMARK.replace('\\', r"\\");
    let out = maps_model(&format!(r#"['{remark}'.parse_json()["remark"] place_details('{remark}')].to_json()"#));
    assert_eq!(
        out,
        serde_json::json!([r#"runtime error: Query timed out in "query" at line 1 after 10 seconds."#, null])
    );
    // An empty remark, a remark with an element (a partial answer still
    // says something), and Overpass's busy page.
    let out = maps_model(
        r#"[place_details('{"elements":[],"remark":""}')
            place_details('{"elements":[{"tags":{"opening_hours":"24/7"}}],"remark":"runtime error: out of memory"}')
            place_details('<?xml version="1.0" encoding="UTF-8"?><html><head><title>OSM3S Response</title></head><body><p><strong style="color:#FF0000">Error</strong>: runtime error: open64: 0 Success /osm3s_osm_base Dispatcher_Client::request_read_and_idx::timeout. The server is probably too busy to handle your request. </p></body></html>')].to_json()"#,
    );
    let blank = serde_json::json!({"hours": "", "phone": "", "website": "", "cuisine": ""});
    assert_eq!(
        out,
        serde_json::json!([blank, {"hours": "24/7", "phone": "", "website": "", "cuisine": ""}, null])
    );
}

/// Laid out as Overpass sends it: indented, with its header, and URLs whose
/// `/` is not escaped. The tags are a real restaurant's, trimmed, with a
/// `contact:phone` and `contact:website` added (`phone` and `website` win)
/// and a second cuisine.
const REAL_OVERPASS: &str = r#"{
  "version": 0.6,
  "generator": "Overpass API 0.7.62.11 87bfad18",
  "osm3s": {
    "timestamp_osm_base": "2026-10-07T21:16:08Z",
    "copyright": "The data included in this document is from www.openstreetmap.org. The data is made available under ODbL."
  },
  "elements": [

{
  "type": "node",
  "id": 2109330888,
  "tags": {
    "addr:street": "Santana Row",
    "amenity": "restaurant",
    "contact:facebook": "zazilcocinamexicana",
    "contact:phone": "+1 408-000-0000",
    "contact:website": "https://contact.example.com/",
    "cuisine": "mexican; latin_american",
    "image": "http://www.santanarow.com/images/vendor/Zazil-1.jpg",
    "name": "Zazil",
    "opening_hours": "Mo-Th 11:30-22:00; Fr 11:30-23:30; Sa 10:30-23:30; Su 10:30-22:00",
    "phone": "+1 408-564-4162",
    "website": "https://zazilsantanarow.com/"
  }
}

  ]
}
"#;

#[test]
fn maps_reads_odd_overpass_answers() {
    let d = maps_model(&format!("place_details('{REAL_OVERPASS}').to_json()"));
    assert_eq!(
        d,
        serde_json::json!({"hours": "Mo-Th 11:30-22:00; Fr 11:30-23:30; Sa 10:30-23:30; Su 10:30-22:00",
            "phone": "+1 408-564-4162", "website": "https://zazilsantanarow.com/",
            "cuisine": "mexican, latin american"})
    );
    // A way (Overpass adds its `center`) with two phones and an upper-case
    // scheme, only `contact:website`, a website that is not a web address, a
    // place without tags (Overpass leaves `tags` out), tags that are not an
    // object, and answers that are not Overpass's; `maps_model` also fails on
    // any script error they raise.
    let odd = maps_model(
        r#"[place_details('{"version":0.6,"elements":[{"type":"way","id":25904339,"center":{"lat":37.3209796,"lon":-121.9486002},"tags":{"name":"Santana Row","opening_hours":"Mo-Sa 10:00-21:00","phone":"+1 408 555 0100; +1 408 555 0101;","website":"HTTPS://Santana.example.com/@row"}}]}')
            place_details('{"elements":[{"tags":{"contact:website":"www.contact.example.com"}}]}')
            place_details('{"elements":[{"tags":{"website":"javascript:alert(1)"}}]}')
            place_details('{"elements":[{"type":"node","id":10735327671}]}')
            place_details('{"elements":[{"tags":"x"}]}')
            place_details('{"elements":["x"]}')
            place_details('{"elements":"x"}')
            place_details('{"elements":{"tags":{"phone":"1"}}}')
            place_details('[1,2]')
            place_details('')].to_json()"#,
    );
    let blank = serde_json::json!({"hours": "", "phone": "", "website": "", "cuisine": ""});
    assert_eq!(
        odd,
        serde_json::json!([
            {"hours": "Mo-Sa 10:00-21:00", "phone": "+1 408 555 0100, +1 408 555 0101",
             "website": "https://Santana.example.com/@row", "cuisine": ""},
            {"hours": "", "phone": "", "website": "https://www.contact.example.com", "cuisine": ""},
            blank, blank, blank, blank, null, null, null, null
        ])
    );
}

#[test]
fn maps_asks_overpass_only_for_openstreetmap_ids() {
    let q = maps_model(r#"[overpass_query("N:123") overpass_query("W:5") overpass_query("R:7") overpass_query("X:1") overpass_query("N:") overpass_query("N:1.5") overpass_query("") cache_name("W:5")].to_json()"#);
    assert_eq!(
        q,
        serde_json::json!([
            "[out:json][timeout:10];node(123);out tags;",
            "[out:json][timeout:10];way(5);out tags center;",
            "[out:json][timeout:10];rel(7);out tags center;",
            "", "", "", "",
            "place_W_5.json"
        ])
    );
    // Only digits, as written: `to_f64` would take "1e3" and " 12".
    let odd = maps_model(
        r#"[overpass_query("N:-3") overpass_query("N:12a") overpass_query("N:1e3") overpass_query("N: 12")
            overpass_query("N:1:2") overpass_query("n:5") overpass_query("N") overpass_query("N:10735327671")
            cache_name("N:10735327671") cache_name("N/../../x:1") cache_name("")].to_json()"#,
    );
    assert_eq!(
        odd,
        serde_json::json!([
            "", "", "", "", "", "", "",
            "[out:json][timeout:10];node(10735327671);out tags;",
            "place_N_10735327671.json", "", ""
        ])
    );
}

#[test]
fn maps_opens_websites_over_https() {
    let u = maps_model(r#"[site_url("www.example.com") site_url("http://a.example.com") site_url("https://b.example.com") site_url("ftp://c.example.com") site_url("")].to_json()"#);
    assert_eq!(
        u,
        serde_json::json!(["https://www.example.com", "https://a.example.com", "https://b.example.com", "", ""])
    );
    // Nothing but a web page opens in the reader.
    let odd = maps_model(
        r#"[site_url("javascript:alert(1)") site_url("data:text/html,<b>x</b>") site_url("mailto:a@example.com")
            site_url("tel:+14085550100") site_url("http://") site_url("https://") site_url(" www.a.example.com ; www.b.example.com")
            site_url("example.com/a:b")].to_json()"#,
    );
    assert_eq!(
        odd,
        serde_json::json!(["", "", "", "", "", "", "https://www.a.example.com", "https://example.com/a:b"])
    );
    // A scheme in any case (only the scheme is lowered), "//", an empty host,
    // and characters parsers read differently: a newline, a space, a
    // backslash, a `%` or anything but ASCII in the host. `@`, `%` and
    // non-ASCII in the path are kept; which hosts may open is the reader's
    // to decide.
    let more = maps_model(
        r#"[site_url("Https://Example.com") site_url("HTTP://EXAMPLE.COM/Path") site_url("JavaScript:alert(1)")
            site_url("//example.com") site_url("/x") site_url("https:///x") site_url("?q=1")
            site_url("www.a\nexample.com") site_url("www.a example.com") site_url("192.168.1.1\\@x.example.com")
            site_url("192.168.1。1") site_url("https://münchen.example") site_url("192.168.1%2e1")
            site_url("https://192.168.1.%31/admin")
            site_url("https://medium.com/@user") site_url("https://example.com/café?q=1#é") site_url("example.com?q=1")
            site_url("localhost") site_url("https://example.com/a%20b")].to_json()"#,
    );
    assert_eq!(
        more,
        serde_json::json!([
            "https://Example.com", "https://EXAMPLE.COM/Path", "",
            "https://example.com", "", "", "",
            "", "", "",
            "", "", "",
            "",
            "https://medium.com/@user", "https://example.com/café?q=1#é", "https://example.com?q=1",
            "https://localhost", "https://example.com/a%20b"
        ])
    );
}

#[test]
fn maps_says_how_far_a_place_is() {
    let km = maps_model("[distance_km(37.3350, -121.8850, 37.3209796, -121.9486002)].to_json()");
    let km = km[0].as_f64().unwrap();
    assert!((5.83..5.84).contains(&km), "{km} km downtown San Jose to Santana Row");
    // The other side of the earth: half its circumference, not NaN. And
    // next to the pole, where f32 rounding once gave NaN too.
    let far = maps_model("[distance_km(37.335, -121.885, -37.335, 58.115) distance_km(90, 0, 89.99999, 180) < 0.01].to_json()");
    let km_far = far[0].as_f64().unwrap();
    assert!((20000.0..20016.0).contains(&km_far), "{km_far} km to the antipode");
    assert_eq!(far[1], serde_json::json!(true), "a meter from the pole");
    let t = maps_model(r#"[distance_text(0.354) distance_text(2.44) distance_text(12.7) coords_text(37.33501, -121.88499)].to_json()"#);
    assert_eq!(t, serde_json::json!(["350 m", "2.4 km", "13 km", "37.335, -121.885"]));
    // A place to itself, the boundaries between meters, tenths of a
    // kilometer and whole kilometers, and a coordinate that rounds to zero
    // from below (not "-0").
    let edge = maps_model(
        r#"[distance_km(37.335, -121.885, 37.335, -121.885) == 0 distance_text(0.994) distance_text(0.996) distance_text(0.99499999)
            distance_text(9.96) coords_text(-0.00001, 51.47791)].to_json()"#,
    );
    assert_eq!(edge, serde_json::json!([true, "990 m", "1 km", "1 km", "10 km", "0, 51.4779"]));
}

#[test]
fn maps_asks_photon_near_the_visible_map() {
    let u = maps_model(r#"[search_url("Santana Row", 37.33501, -121.88499) reverse_url(37.33501, -121.88499)].to_json()"#);
    assert_eq!(
        u,
        serde_json::json!([
            "https://photon.komoot.io/api/?q=Santana%20Row&limit=8&lang=en&lat=37.335&lon=-121.885",
            "https://photon.komoot.io/reverse?lat=37.335&lon=-121.885&lang=en&limit=1"
        ])
    );
    // What the reader typed stays inside `q`: `&` and `#` are percent-encoded,
    // and so is each byte of a letter outside ASCII.
    let odd = maps_model(r#"[search_url("A&B #1", 37.33501, -121.88499) search_url("Café", 37.33501, -121.88499)].to_json()"#);
    assert_eq!(
        odd,
        serde_json::json!([
            "https://photon.komoot.io/api/?q=A%26B%20%231&limit=8&lang=en&lat=37.335&lon=-121.885",
            "https://photon.komoot.io/api/?q=Caf%C3%A9&limit=8&lang=en&lat=37.335&lon=-121.885"
        ])
    );
}

#[test]
fn maps_saves_a_place_once_and_pins_the_open_one_last() {
    let out = maps_model(
        r#"let a = {id: "W:1" name: "A" cat: "" label: "" lat: 37.1 lon: -121.1}
let b = {name: "B" cat: "" label: "" lat: 37.2 lon: -121.2}
let s = with_saved(with_saved(with_saved([], a), b), a)
[s.len() is_saved(s, b) without_saved(s, a).len() pins_text(pin_places(s, b), b) pins_text(pin_places(s, nil), nil)].to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([2, true, 1, "37.1,-121.1,1;37.2,-121.2,2", "37.2,-121.2,1;37.1,-121.1,1"])
    );
    // MapView draws only places (in range, not 0,0) and counts only the pins
    // it draws, so `on_marker(i)` means `pin_places(...)[i]`: 0,0, a latitude
    // of 95 and a longitude of 181 are in neither list, while a pole on the
    // date line and a point on the equator are. An open place that isn't a
    // place is not pinned.
    let out = maps_model(
        r#"let a = {id: "W:1" name: "A" cat: "" label: "" lat: 37.1 lon: -121.1}
let b = {name: "B" cat: "" label: "" lat: 37.2 lon: -121.2}
let zero = {name: "Zero" cat: "" label: "" lat: 0 lon: 0}
let north = {name: "North" lat: 95 lon: -121.3}
let pole = {name: "Pole" lat: -90 lon: 180}
let equator = {name: "Equator" lat: 0 lon: 9.5}
let east = {name: "East" lat: 37.3 lon: 181}
let s = [zero a north pole equator east b]
let names = []
for p in pin_places(s, a) { names.push(p.name) }
let unopened = []
for p in pin_places(s, zero) { unopened.push(p.name) }
[names pins_text(pin_places(s, a), a) unopened pins_text(pin_places(s, zero), zero)].to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([
            ["Pole", "Equator", "B", "A"], "-90,180,1;0,9.5,1;37.2,-121.2,1;37.1,-121.1,2",
            ["A", "Pole", "Equator", "B"], "37.1,-121.1,1;-90,180,1;0,9.5,1;37.2,-121.2,1"
        ])
    );
    // A `saved.json` from an older or broken build: a bare number, null, text,
    // a place without coordinates, one with them as text, one past the
    // largest number (it reads as infinity) and bare `inf`/`NaN` words stay
    // in the list but are never pinned; `maps_model` also fails on any script
    // error they raise. A bare word read as a name is no name. Saving keeps
    // only a place's own fields, and a coordinate only when it is a finite
    // number, so the app never writes `inf` or `NaN` itself.
    let out = maps_model(
        r#"let disk = '[3,null,"x",{"name":"X"},{"name":"Text","lat":"37.3","lon":"-121.3"},{"name":"Far","lat":1e999,"lon":1},{"name":"Inf","lat":inf,"lon":1},{"name":"Nan","lat":NaN,"lon":1},{"name":NaN,"lat":1,"lon":1},{"id":"W:1","name":"A","lat":37.1,"lon":-121.1}]'.parse_json()
let b = {name: "B" cat: "" label: "" lat: 37.2 lon: -121.2}
let names = []
for p in pin_places(disk, b) { names.push(text_of(p, "name")) }
[is_saved(disk, {id: "W:1"}) is_saved(disk, b) names pins_text(pin_places(disk, b), b)
    with_saved(disk, b).len() without_saved(disk, {name: "X"}).len()
    with_saved([], {name: " Q " lat: 1.5 lon: 2 extra: true})
    with_saved([], {name: "Inf" lat: 1 / 0 lon: 0 / 0}).to_json()].to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([
            true, false, ["", "A", "B"], "1,1,1;37.1,-121.1,1;37.2,-121.2,2", 11, 9,
            [{"id": "", "name": "Q", "cat": "", "label": "", "lat": 1.5, "lon": 2}],
            r#"[{"id":"","name":"Inf","cat":"","label":"","lat":null,"lon":null}]"#
        ])
    );
    // MapView's own boundaries: 1e-9 from 0,0 is a place, just under it is
    // not, nor is a coordinate that isn't a number. A place saved twice gets
    // one pin. When the open place can't be pinned, nothing is drawn as the
    // open one, not even a saved place with its id.
    let out = maps_model(
        r#"let tiny = {name: "Tiny" lat: 0.000000001 lon: 0}
let under = {name: "Under" lat: 0.0000000009999 lon: 0}
let minus = {name: "Minus" lat: -0.0 lon: 5}
let word = {name: "Word" lat: "x".to_number() lon: 1}
let a = {id: "W:1" name: "A" lat: 37.1 lon: -121.1}
let twin = {id: "W:1" name: "A again" lat: 37.2 lon: -121.2}
let lost = {id: "W:1" name: "Lost" lat: "37.1" lon: -121.1}
let s = [tiny under minus word a twin]
let names = []
for p in pin_places(s, nil) { names.push(p.name) }
[names pins_text(pin_places(s, nil), nil) pins_text(pin_places(s, lost), lost)].to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([
            ["Tiny", "Minus", "A"], "0.000000001,0,1;-0,5,1;37.1,-121.1,1", "0.000000001,0,1;-0,5,1;37.1,-121.1,1"
        ])
    );
}

#[test]
fn maps_tells_its_own_flights_landing_from_the_persons_move() {
    // MapView reports where a flight landed through its Mercator round trip,
    // well within 1e-12 degrees of the target. Within 1e-6 degrees on both
    // axes it is Maps' own landing; 2e-6 off on either axis, elsewhere, or
    // with no flight of Maps' own under way, the person moved the map.
    let out = maps_model(
        r#"let t = {lat: 37.3349 lon: -121.8851}
[own_landing(t, 37.3349, -121.8851) own_landing(t, 37.334900000001, -121.885099999999)
    own_landing(t, 37.3349005, -121.8851005) own_landing(t, 37.334902, -121.8851)
    own_landing(t, 37.3349, -121.885098) own_landing(t, -37.3349, 121.8851)
    own_landing(nil, 37.3349, -121.8851)].to_json()"#,
    );
    assert_eq!(out, serde_json::json!([true, true, true, false, false, false, false]));
    // The script subtracts in f64: in f32 one step at 37 degrees is 3.8e-6,
    // and the 5e-7 between these two coordinates would read 0 or 3.8e-6.
    let d = maps_model("[37.3349005 - 37.3349].to_json()")[0].as_f64().unwrap();
    assert!(d > 4.9e-7 && d < 5.1e-7, "{d}");
}

#[test]
fn maps_closes_the_list_when_the_person_moves_the_map_not_when_its_own_flight_lands() {
    // The browse map's callbacks, with `ui` stubbed: whether the results
    // list shows after each, and the flights Maps starts.
    let out = maps_model(
        r#"mod.visible = nil
mod.flights = []
let ui = {results: {set_visible: fn(v) {mod.visible = v}}
    browse_map: {fly_to: fn(lat, lon, zoom) {mod.flights.push([lat lon zoom])}}}
let log = []
// With the box empty and nothing in Saved or Recent, there is no list to open.
show_results(true)
log.push(mod.visible)
// Nor when their files hold only entries that show no row.
saved = list_in('[3,{"lat":37.1,"lon":-121.1}]')
recents = list_in('[inf,{"name":"X"}]')
show_results(true)
log.push(mod.visible)
// A recent place the list can show (a nameless or unplaced entry has no row).
recents = [{name: "A" lat: 37.1 lon: -121.1}]
// Opened while Maps flies to the fix: its landing leaves the list open and
// is not the person's move.
show_results(true)
fly(37.7749, -122.4194, 14)
viewport_moved(37.7749, -122.4194, 14)
log.push([mod.visible centered])
// Once landed, the same centre again (a zoom) is the person's.
viewport_moved(37.7749, -122.4194, 15)
log.push([mod.visible centered])
show_results(true)
map_tapped(37.7, -122.4)
log.push(mod.visible)
// A tap that stops a flight reports no landing; the next pan still closes,
// and the tap forgets the flight's target.
fly(40.7128, -74.006, 15)
show_results(true)
map_tapped(40.7, -74.0)
log.push(app_target)
show_results(true)
viewport_moved(40.7, -74.01, 15)
log.push(mod.visible)
// Off the search screen the map leaves the list alone.
screen = "place"
mod.visible = "untouched"
map_tapped(1, 2)
viewport_moved(3, 4, 15)
log.push(mod.visible)
// Choosing a start with nothing in Saved or Recent: no list under the way
// back, which sits above it (a render of nothing would keep old rows).
screen = "origin"
saved = []
recents = []
show_results(true)
log.push(mod.visible)
// MapView would ignore a flight to a point that isn't a place.
fly(0, 0, 14)
[log mod.flights seen].to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([
            [false, false, [true, false], [false, true], false, null, false, "untouched", false],
            [[37.7749, -122.4194, 14], [40.7128, -74.006, 15]],
            {"lat": 3, "lon": 4}
        ])
    );
}

#[test]
fn maps_asks_for_location_only_from_its_button_and_the_fix_never_moves_a_map_the_person_moved() {
    // `center_on_fix` runs on the timer: it only reads the fix. Only ◎
    // asks for one. A fix that comes after the person moved the map moves
    // nothing, but ends ◎'s "Waiting for location…".
    let out = maps_model(
        r#"mod.flights = []
mod.hint = ""
mod.requests = 0
mod.fixed = false
let ui = {results: {set_visible: fn(v) {}}
    search_hint: {set_text: fn(t) {mod.hint = t}}
    browse_map: {fly_to: fn(lat, lon, zoom) {mod.flights.push([lat lon zoom])}}}
let sys = {request_location: fn() {mod.requests = mod.requests + 1; return 1}
    gps: fn(field) {if !mod.fixed {return 0}; if field == "lat" {return 37.7}; if field == "lon" {return -122.4}; return 1}}
let log = []
center_on_fix()
log.push([mod.requests mod.flights.len()])
locate()
log.push([mod.requests mod.hint])
viewport_moved(37.0, -121.0, 13)
mod.fixed = true
center_on_fix()
log.push([mod.requests mod.flights.len() mod.hint])
locate()
log.push([mod.requests mod.flights])
log.to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([[0, 0], [1, "Waiting for location…"], [1, 0, "Where to?"], [1, [[37.7, -122.4, 15]]]])
    );
}

#[test]
fn maps_shows_only_named_places_from_saved_and_recent_files() {
    // A file of ours that holds no list reads as an empty one; a truncated
    // list keeps what parsed, and none of it shows.
    let out = maps_model(
        r#"[list_in('') list_in('inf') list_in('null') list_in('3') list_in('"x"') list_in('{"name":"A","lat":1,"lon":2}')
    listed(list_in('[1,')) listed(nil) listed("x") listed({name: "A" lat: 1 lon: 2})].to_json()"#,
    );
    assert_eq!(out, serde_json::json!([[], [], [], [], [], [], [], [], [], []]));
    // An older or broken build's entries stay in the list but show no row:
    // bare words, a number, null and text, a place without a name or with a
    // blank or bare-word one, one without coordinates or with them as text,
    // a bare `inf`, 0,0 or out of range. A row reads its name and address
    // safely (an entry from before ids has no `id`, `cat` or `label`), and
    // its tap gets every field of a place; `maps_model` also fails on any
    // script error they raise.
    let out = maps_model(
        r#"let disk = list_in('[inf,3,null,"x",{"name":"X"},{"lat":37.1,"lon":-121.1},{"name":"  ","lat":37.1,"lon":-121.1},{"name":NaN,"lat":1,"lon":1},{"name":"Inf","lat":inf,"lon":1},{"name":"Text","lat":"37.3","lon":"-121.3"},{"name":"Zero","lat":0,"lon":0},{"name":"North","lat":95,"lon":1},{"name":"A","lat":37.1,"lon":-121.1},{"id":"W:1","name":"B","cat":"Retail","label":"San Jose","lat":37.2,"lon":-121.2}]')
let rows = []
let picked = []
for s in listed(disk) {
    rows.push([text_of(s, "name") text_of(s, "label")])
    picked.push(as_place(s))
}
[disk.len() rows picked].to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([
            14,
            [["A", ""], ["B", "San Jose"]],
            [{"id": "", "name": "A", "cat": "", "label": "", "lat": 37.1, "lon": -121.1},
             {"id": "W:1", "name": "B", "cat": "Retail", "label": "San Jose", "lat": 37.2, "lon": -121.2}]
        ])
    );
}

#[test]
fn maps_opens_a_recent_place_from_an_older_build_with_every_field() {
    // A recent place from before ids has no `id`, `cat` or `label`. Its tap
    // opens the card, which reads each field (`place.cat` raises on a missing
    // one), and remembers it whole. Without an OpenStreetMap id it asks
    // Overpass nothing and reads no cache. `ui`, `host`, `sys`, `fs` and
    // `net` stubbed.
    let out = maps_model(
        r#"mod.texts = {}
mod.writes = []
mod.urls = []
mod.flights = []
fn text_field(id){ return {set_text: fn(t) { mod.texts[id] = t } set_visible: fn(v) {} render: fn() {}} }
let widget = {set_visible: fn(v) {} set_text: fn(t) {} render: fn() {}}
let ui = {results: widget search_panel: widget place_panel: widget route_panel: widget browse_box: widget
    locate_box: widget drive_box: widget drive_bar: widget details: widget
    browse_map: {fly_to: fn(lat, lon, zoom) { mod.flights.push([lat lon zoom]) } set_route_markers: fn(text) {}}
    pname: text_field("pname") pcat: text_field("pcat") paddr: text_field("paddr") pdist: text_field("pdist")
    save: text_field("save")}
let host = {has: fn(capability) { false }}
let sys = {gps: fn(field) { 0 } navroute: fn(a, b, c, d, field, v) { "—" }}
let fs = {exists: fn(path) { true } write: fn(path, data) { mod.writes.push([path data]) }}
let net = {HttpMethod: {GET: "GET"} HttpRequest: {} HttpEvents: {}
    http_request: fn(req, events) { mod.urls.push(req.url); events.on_error("offline") }}
recents = list_in('[{"name":"Old Recent","lat":37.335,"lon":-121.885}]')
for r in listed(recents) { pick(r) }
[screen place mod.texts mod.writes.len() mod.writes[0][0] mod.writes[0][1].parse_json() mod.urls mod.flights].to_json()"#,
    );
    let old = serde_json::json!({"id": "", "name": "Old Recent", "cat": "", "label": "", "lat": 37.335, "lon": -121.885});
    assert_eq!(
        out,
        serde_json::json!([
            "place", old, {"pname": "Old Recent", "pcat": "", "paddr": "", "pdist": "", "save": "Save"},
            1, "accounts/device/recents.json", [old], [], [[37.335, -121.885, 16]]
        ])
    );
}

#[test]
fn maps_keeps_the_way_back_above_the_list_while_choosing_a_start_or_a_stop() {
    // The list scrolls, and keeps its place when the screen changes: "◎ Your
    // location" and "‹ Back to route" sit above it, so they never scroll
    // out of sight. `ui`, `host` and `sys` stubbed.
    let out = maps_model(
        r#"mod.shown = {}
fn shown(id){ return {set_visible: fn(v) { mod.shown[id] = v }} }
let widget = {set_visible: fn(v) {} set_text: fn(t) {} render: fn() {}}
let ui = {results: widget search: widget search_hint: widget search_panel: widget place_panel: widget
    route_panel: widget browse_box: widget locate_box: widget drive_box: widget drive_bar: widget
    finding_links: shown("finding_links") your_location: shown("your_location")
    browse_map: {set_route_markers: fn(text) {}}}
let host = {has: fn(capability) { false }}
let sys = {gps: fn(field) { 0 }}
let log = []
for s in ["origin" "stop" "search"] {
    show(s)
    log.push([s mod.shown["finding_links"] mod.shown["your_location"]])
}
log.to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([["origin", true, true], ["stop", true, false], ["search", false, false]])
    );
}

#[test]
fn maps_remembers_a_place_once_and_the_last_eight() {
    // Recent places: the newest last, once by name, at most 8. Entries from
    // a broken file (a bare `inf` first, a place without a name, text, null,
    // a number) count toward the 8 and roll off the front like the others,
    // and a nameless one is never taken for the new place.
    let out = maps_model(
        r#"fn names_of(list){
    let out = []
    for r in list { out.push(text_of(r, "name")) }
    out
}
let disk = list_in('[inf,{"lat":1},{"name":"A","lat":1,"lon":1},"x",null,3,{"name":"B","lat":2,"lon":2},{"name":"C","lat":3,"lon":3}]')
let a = {id: "" name: "A" cat: "" label: "" lat: 5 lon: 5}
let d = {id: "N:4" name: "D" cat: "" label: "" lat: 4 lon: 4}
let once = remembered(disk, a)
let twice = remembered(once, d)
let again = remembered(twice, {name: "B" lat: 6 lon: 6})
[names_of(once) names_of(twice) names_of(listed(twice)) names_of(listed(again)) listed(again)[3].lat].to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([
            ["", "", "", "", "", "B", "C", "A"],
            ["", "", "", "", "B", "C", "A", "D"],
            ["B", "C", "A", "D"],
            ["C", "A", "D", "B"],
            6
        ])
    );
}

/// Maps' search with the runtime stubbed: `ui` (the list's visibility is
/// `mod.visible`), `host`, `sys`, and the request under `fetch`. Each
/// request's answer is the next of `mod.answers` (nil: the request failed;
/// "refused": the runtime refuses it, raising as it does for a host off the
/// manifest's list), and `mod.during` is what the person does while it is
/// on its way.
const SEARCH_STUBS: &str = r#"mod.visible = nil
mod.urls = []
mod.agents = []
mod.answers = []
mod.during = nil
let widget = {set_visible: fn(v) {} set_text: fn(text) {} render: fn() {}}
let ui = {results: {set_visible: fn(v) {mod.visible = v} render: fn() {}}
    search: widget search_hint: widget search_panel: widget place_panel: widget route_panel: widget
    browse_box: widget locate_box: widget drive_box: widget drive_bar: widget
    finding_links: widget your_location: widget}
let host = {has: fn(capability) { false }}
let sys = {gps: fn(field) { 0 }}
let net = {HttpMethod: {GET: "GET"} HttpRequest: {} HttpEvents: {}
    http_request: fn(req, events) {
        mod.urls.push(req.url)
        mod.agents.push(req.headers["User-Agent"])
        let res = mod.answers[0]
        mod.answers.remove(0)
        if res == "refused" { refuse_the_request() }
        if res == nil { events.on_error("offline") } else { events.on_response(res) }
    }}
// A promise nothing resolves would wait for good: here it raises instead.
fn promise(){
    let held = {value: nil resolved: false}
    let act = mod.during
    mod.during = nil
    return {resolve: fn(v) { held.value = v; held.resolved = true } await: fn() {
        if act != nil { act() }
        if !held.resolved { never_answered() }
        held.value
    }}
}
fn state(){ return [q search_state hits.len() mod.visible list_open] }
recents = [{name: "R" lat: 37.1 lon: -121.1}]
"#;

#[test]
fn maps_shows_only_the_newest_searchs_answer_and_leaves_the_list_as_the_person_left_it() {
    let code = r#"seen = {lat: 40.7128 lon: -74.006}
let places = {status_code: 200 body: 'PHOTON_BODY'}
let none = {status_code: 200 body: '{"features":[]}'}
let busy = {status_code: 503 body: 'PHOTON_BODY'}
let missing = {status_code: 404 body: 'PHOTON_BODY'}
let bodiless = {status_code: 200 body: nil}
let page = {status_code: 200 body: '<html>busy</html>'}
let log = []
// A search near the visible map.
mod.answers = [places]
search_for(" Pizza ")
log.push(state())
// The box emptied while the answer is on its way: Saved and Recent show,
// and the late answer is dropped.
mod.answers = [places]
mod.during = fn() { search_for("") }
search_for("Sushi")
log.push(state())
// A newer search while one is on its way: only the newer answer shows.
mod.answers = [places none]
mod.during = fn() { search_for("Tacos") }
search_for("Pizza")
log.push(state())
// The list closed while the answer is on its way: the answer fills it, and
// it stays closed.
mod.answers = [places]
mod.during = fn() { map_tapped(40.7, -74.0) }
search_for("Pizza")
log.push(state())
// A start chosen while the answer is on its way: its list stays empty.
mod.answers = [places]
mod.during = fn() { find_for("origin") }
search_for("Pizza")
log.push(state())
log.push(screen)
screen = "search"
finding = ""
// No answer, an HTTP error (503, or a 404 whose page reads like Photon's),
// no body, not Photon's, or a request the runtime refused: search isn't
// available. Photon with no places: none found.
mod.answers = [nil busy missing bodiless page "refused" none]
let outcomes = []
search_for("Pizza")
outcomes.push(search_state)
search_for("Pizza")
outcomes.push(search_state)
search_for("Pizza")
outcomes.push(search_state)
search_for("Pizza")
outcomes.push(search_state)
search_for("Pizza")
outcomes.push(search_state)
search_for("Pizza")
outcomes.push(search_state)
search_for("Pizza")
outcomes.push([search_state hits.len()])
log.push(outcomes)
[log mod.urls[0] mod.agents[0] mod.urls.len()].to_json()"#
        .replace("PHOTON_BODY", PHOTON);
    let out = maps_model(&format!("{SEARCH_STUBS}{code}"));
    assert_eq!(
        out,
        serde_json::json!([
            [
                ["Pizza", "done", 2, true, true],
                ["", "", 0, true, true],
                ["Tacos", "done", 0, true, true],
                ["Pizza", "done", 2, false, false],
                ["", "", 0, true, true],
                "origin",
                ["failed", "failed", "failed", "failed", "failed", "failed", ["done", 0]]
            ],
            "https://photon.komoot.io/api/?q=Pizza&limit=8&lang=en&lat=40.7128&lon=-74.006",
            "OctoSense-Maps/1.0",
            // An emptied box asks for nothing.
            13
        ])
    );
}

/// Maps' place card with the runtime stubbed. `ui`: each widget's text is
/// `mod.texts[id]` and its visibility `mod.shown[id]`, the details' renders
/// count in `mod.renders`, and the browse map's pins and flights are
/// `mod.markers` and `mod.flights`. `sys`: a fix only with `mod.fix`, and
/// every route field is `mod.line` ("—": the route is still loading).
/// `fs`: files are `mod.files[path]`, and
/// every call is logged in `mod.io` (a read of a missing file raises, as the
/// runtime's does). The clock is `mod.now`. Requests go as in SEARCH_STUBS:
/// each takes the next of `mod.answers` (nil, or none left: the request
/// failed; "refused": the runtime refuses it), and `mod.during` is what
/// happens while one is on its way.
const CARD_STUBS: &str = r#"mod.texts = {}
mod.shown = {}
mod.renders = 0
mod.markers = []
mod.flights = []
mod.files = {}
mod.io = []
mod.urls = []
mod.answers = []
mod.during = nil
mod.now = 1800000000
mod.fix = false
mod.line = "—"
fn w(id){ return {set_text: fn(t) { mod.texts[id] = t } set_visible: fn(v) { mod.shown[id] = v }
    render: fn() { if id == "details" { mod.renders = mod.renders + 1 } }} }
let ui = {results: w("results") search: w("search") search_hint: w("search_hint") search_panel: w("search_panel")
    place_panel: w("place_panel") route_panel: w("route_panel") browse_box: w("browse_box") locate_box: w("locate_box")
    drive_box: w("drive_box") drive_bar: w("drive_bar") finding_links: w("finding_links") your_location: w("your_location")
    pname: w("pname") pcat: w("pcat") paddr: w("paddr") pdist: w("pdist") details: w("details") save: w("save")
    location_status: w("location_status") rfrom: w("rfrom") rto: w("rto") modes: w("modes") stop_rows: w("stop_rows")
    reta: w("reta") rdist: w("rdist")
    browse_map: {fly_to: fn(lat, lon, zoom) { mod.flights.push([lat lon zoom]) } set_route_markers: fn(text) { mod.markers.push(text) }}}
let host = {has: fn(capability) { false }}
// No fix unless `mod.fix`; then downtown San Jose.
let sys = {gps: fn(field) { if !mod.fix { return 0 }; if field == "lat" { return 37.3350 }; if field == "lon" { return -121.8850 }; return 1 }
    navroute: fn(lat1, lon1, lat2, lon2, field, vias) { mod.line }}
fn time_now(){ mod.now }
let fs = {
    exists: fn(path) { mod.io.push("exists " + path); return optional(mod.files, path, nil) != nil }
    read: fn(path) {
        mod.io.push("read " + path)
        let text = optional(mod.files, path, nil)
        if text == nil { no_such_file() }
        text
    }
    write: fn(path, data) { mod.io.push("write " + path); mod.files[path] = data }
}
let net = {HttpMethod: {GET: "GET"} HttpRequest: {} HttpEvents: {}
    http_request: fn(req, events) {
        mod.urls.push(req.url)
        let res = nil
        if mod.answers.len() > 0 {
            res = mod.answers[0]
            mod.answers.remove(0)
        }
        if res == "refused" { refuse_the_request() }
        if res == nil { events.on_error("offline") } else { events.on_response(res) }
    }}
fn promise(){
    let held = {value: nil resolved: false}
    let act = mod.during
    mod.during = nil
    return {resolve: fn(v) { held.value = v; held.resolved = true } await: fn() {
        if act != nil { act() }
        if !held.resolved { never_answered() }
        held.value
    }}
}
// The host each request went to, in order.
fn hosts(){
    let out = []
    for u in mod.urls { out.push(u.split("/api/")[0]) }
    out
}
fn kept(name){
    let text = optional(mod.files, "cache/" + name, nil)
    if text == nil { return nil }
    text.parse_json()
}
"#;

const PIZZA_DETAILS: &str = r#"{"elements":[{"type":"node","id":1,"tags":{"opening_hours":"Mo-Su 11:00-22:00","phone":"+1 408 555 0100","website":"pizza.example.com","cuisine":"pizza"}}]}"#;
const SUSHI_DETAILS: &str = r#"{"elements":[{"type":"node","id":3,"tags":{"opening_hours":"Tu-Su 17:00-22:00","cuisine":"sushi"}}]}"#;

#[test]
fn maps_keeps_only_overpass_answers_and_tries_the_next_mirror_on_anything_else() {
    let code = r#"let pizza = {id: "N:1" name: "Pizza" cat: "Restaurant" label: "" lat: 37.1 lon: -121.1}
let gateway = {status_code: 504 body: '<html>504 Gateway Time-out</html>'}
let timed_out = {status_code: 200 body: '{"elements":[],"remark":"runtime error: Query timed out"}'}
let busy = {status_code: 200 body: '<?xml version="1.0"?><html><body>The server is probably too busy</body></html>'}
let answer = {status_code: 200 body: 'PIZZA'}
let log = []
// No answer, a 504 page, and Overpass's own timeout (no elements, a
// remark): every mirror is asked once, nothing is shown and nothing is kept.
// (Each answer is named: `[nil {…}]` would read as one value.)
mod.answers = [nil gateway timed_out]
open_place(pizza)
log.push([hosts() detail mod.shown["details"] kept("place_N_1.json")])
mod.urls = []
// A request the runtime refused, Overpass's busy page with a 200, then the
// last mirror's real answer: it shows, and is kept with the time.
mod.answers = ["refused" busy answer]
open_place(pizza)
log.push([hosts() detail mod.shown["details"] mod.renders kept("place_N_1.json")])
// The same place again: from the cache, with no request.
mod.urls = []
close_place()
open_place(pizza)
log.push([mod.urls.len() detail.hours mod.shown["details"]])
// A real "no details" answer is an answer: kept, and the details hidden.
mod.urls = []
mod.answers = [{status_code: 200 body: '{"version":0.6,"elements":[]}'}]
open_place({id: "W:2" name: "Park" cat: "Park" label: "" lat: 37.2 lon: -121.2})
log.push([hosts() detail mod.shown["details"] kept("place_W_2.json")])
log.to_json()"#
        .replace("PIZZA", PIZZA_DETAILS);
    let out = maps_model(&format!("{CARD_STUBS}{code}"));
    let blank = serde_json::json!({"hours": "", "phone": "", "website": "", "cuisine": ""});
    let pizza = serde_json::json!({"hours": "Mo-Su 11:00-22:00", "phone": "+1 408 555 0100",
        "website": "https://pizza.example.com", "cuisine": "pizza"});
    let mirrors = ["https://overpass-api.de", "https://overpass.kumi.systems", "https://overpass.openstreetmap.fr"];
    assert_eq!(
        out,
        serde_json::json!([
            [mirrors, blank, false, null],
            [mirrors, pizza, true, 1, {"at": 1800000000, "detail": pizza}],
            [0, "Mo-Su 11:00-22:00", true],
            [&mirrors[..1], blank, false, {"at": 1800000000, "detail": blank}]
        ])
    );
}

#[test]
fn maps_drops_the_details_of_a_card_that_moved_on_and_asks_nothing_for_a_place_without_an_id() {
    let code = r#"let pizza = {id: "N:1" name: "Pizza" cat: "Restaurant" label: "" lat: 37.1 lon: -121.1}
let sushi = {id: "N:3" name: "Sushi" cat: "Restaurant" label: "" lat: 37.3 lon: -121.3}
let pizza_answer = {status_code: 200 body: 'PIZZA'}
let sushi_answer = {status_code: 200 body: 'SUSHI'}
let log = []
// Sushi opened while Pizza's answer is on its way: only Sushi's shows, and
// Pizza's is neither kept nor asked of another mirror.
mod.answers = [pizza_answer sushi_answer]
mod.during = fn() { open_place(sushi) }
open_place(pizza)
log.push([mod.urls.len() place.name detail kept("place_N_1.json") == nil kept("place_N_3.json") != nil])
// Closed while the first mirror fails: the next isn't asked.
mod.urls = []
mod.answers = [nil pizza_answer]
mod.during = fn() { close_place() }
open_place(pizza)
log.push([mod.urls.len() place screen kept("place_N_1.json") == nil])
// No OpenStreetMap id, or one `cache_name` rejects: no request and no file
// touched (`cache_path("")` would be the cache folder itself). Nor for a
// lookup whose card is no longer open.
mod.urls = []
mod.io = []
for id in ["" "X:1" "N:1.5" "N/../../x:1" "N:1:2"] {
    open_place({id: id name: "Spot" cat: "" label: "" lat: 37.4 lon: -121.4})
}
// A recent place from before ids, as a row's tap copies it.
open_place(as_place({name: "No id" lat: 37.4 lon: -121.4}))
load_details(pizza, card_seq - 1)
log.push([mod.urls.len() mod.io detail mod.shown["details"]])
log.to_json()"#
        .replace("PIZZA", PIZZA_DETAILS)
        .replace("SUSHI", SUSHI_DETAILS);
    let out = maps_model(&format!("{CARD_STUBS}{code}"));
    let blank = serde_json::json!({"hours": "", "phone": "", "website": "", "cuisine": ""});
    let sushi = serde_json::json!({"hours": "Tu-Su 17:00-22:00", "phone": "", "website": "", "cuisine": "sushi"});
    assert_eq!(
        out,
        serde_json::json!([
            [2, "Sushi", sushi, true, true],
            [1, null, "search", true],
            [0, [], blank, false]
        ])
    );
}

#[test]
fn maps_reads_kept_details_only_when_fresh_and_whole() {
    let code = r#"let day = 86400
fn keep(name, text){ mod.files["cache/" + name] = text }
let good = '"detail":{"hours":"24/7","phone":"1","website":"http://ok.example.com","cuisine":"pizza"}'
keep("fresh.json", '{"at":' + (mod.now - day) + ',' + good + '}')
keep("stale.json", '{"at":' + (mod.now - 8 * day) + ',' + good + '}')
keep("future.json", '{"at":' + (mod.now + day) + ',' + good + '}')
keep("no_at.json", '{' + good + '}')
keep("text_at.json", '{"at":"' + (mod.now - day) + '",' + good + '}')
keep("nan_at.json", '{"at":NaN,' + good + '}')
keep("truncated.json", '{"at":' + (mod.now - day) + ',"detail":{"hours":')
keep("list.json", '[1,2]')
keep("null.json", 'null')
keep("text_detail.json", '{"at":' + (mod.now - day) + ',"detail":"x"}')
keep("list_detail.json", '{"at":' + (mod.now - day) + ',"detail":[1]}')
keep("missing_field.json", '{"at":' + (mod.now - day) + ',"detail":{"hours":"24/7","phone":"1","website":""}}')
keep("number_field.json", '{"at":' + (mod.now - day) + ',"detail":{"hours":"24/7","phone":5550100,"website":"","cuisine":""}}')
keep("bad_site.json", '{"at":' + (mod.now - day) + ',"detail":{"hours":"24/7","phone":"","website":"javascript:alert(1)","cuisine":""}}')
keep("spaced_site.json", '{"at":' + (mod.now - day) + ',"detail":{"hours":"","phone":"","website":"https://a.example.com/a b","cuisine":""}}')
keep("upper_site.json", '{"at":' + (mod.now - day) + ',"detail":{"hours":"","phone":"","website":"HTTP://Up.example.com","cuisine":"","extra":1}}')
let out = []
for name in ["fresh.json" "stale.json" "future.json" "no_at.json" "text_at.json" "nan_at.json" "truncated.json"
    "list.json" "null.json" "text_detail.json" "list_detail.json" "missing_field.json" "number_field.json"
    "bad_site.json" "spaced_site.json" "upper_site.json" "missing.json"] {
    out.push(cached_detail(name))
}
out.to_json()"#;
    let out = maps_model(&format!("{CARD_STUBS}{code}"));
    let d = |hours: &str, phone: &str, website: &str, cuisine: &str| {
        serde_json::json!({"hours": hours, "phone": phone, "website": website, "cuisine": cuisine})
    };
    assert_eq!(
        out,
        serde_json::json!([
            d("24/7", "1", "https://ok.example.com", "pizza"),
            // A week old, from the future, without a numeric `at`, cut short,
            // not an object, or details that aren't the four texts Maps
            // writes: asked again.
            null, null, null, null, null, null, null, null, null, null, null, null,
            // A website `site_url` refuses opens nothing; the rest is kept.
            d("24/7", "", "", ""),
            d("", "", "", ""),
            d("", "", "https://Up.example.com", ""),
            null
        ])
    );
}

#[test]
fn maps_pins_saved_places_and_the_open_one_from_the_list_it_shows() {
    let code = r#"let a = {id: "W:1" name: "A" cat: "" label: "" lat: 37.1 lon: -121.1}
let b = {id: "" name: "B" cat: "" label: "" lat: 37.2 lon: -121.2}
fn names(){
    let out = []
    for p in shown_pins { out.push(text_of(p, "name")) }
    out
}
// Each step: Save's text, the pins drawn last, the places behind them in pin
// order, and saved.json.
fn step(){
    let file = optional(mod.files, "accounts/device/saved.json", nil)
    let names_saved = []
    if file != nil { for p in list_in(file) { names_saved.push(text_of(p, "name")) } }
    return [mod.texts["save"] mod.markers[mod.markers.len() - 1] names() names_saved]
}
let log = []
show("search")
log.push([mod.markers.len() mod.markers[0]])
open_place(a)
log.push(step())
toggle_save()
log.push(step())
open_place(b)
toggle_save()
log.push(step())
close_place()
log.push(step())
open_place(a)
toggle_save()
log.push(step())
close_place()
log.push(step())
// Directions and the drive keep the route's own pins.
let drawn = mod.markers.len()
screen = "route"
show_pins()
log.push(mod.markers.len() == drawn)
log.to_json()"#;
    let out = maps_model(&format!("{CARD_STUBS}{code}"));
    assert_eq!(
        out,
        serde_json::json!([
            [1, ""],
            ["Save", "37.1,-121.1,2", ["A"], []],
            ["Saved", "37.1,-121.1,2", ["A"], ["A"]],
            ["Saved", "37.1,-121.1,1;37.2,-121.2,2", ["A", "B"], ["A", "B"]],
            // Close keeps the saved pins, none of them the open one.
            ["Saved", "37.1,-121.1,1;37.2,-121.2,1", ["A", "B"], ["A", "B"]],
            // Unsaved while open: still pinned as the open place.
            ["Save", "37.2,-121.2,1;37.1,-121.1,2", ["B", "A"], ["B"]],
            // Closed: its pin is gone.
            ["Save", "37.2,-121.2,1", ["B"], ["B"]],
            true
        ])
    );
}

#[test]
fn maps_opens_a_pins_place_by_its_position_among_the_drawn_pins() {
    let code = r#"// An entry as an older build's saved.json holds it (no id, cat or
// label), one that isn't a place (never pinned), and a saved place.
saved = list_in('[{"name":"Raw","lat":37.3,"lon":-121.3},{"name":"Nowhere"},{"id":"W:1","name":"A","cat":"Mall","label":"San Jose","lat":37.1,"lon":-121.1}]')
let log = []
show("search")
log.push(mod.markers[mod.markers.len() - 1])
marker_tapped(0)
log.push([screen place mod.texts["pname"] mod.texts["pcat"] mod.urls.len() mod.markers[mod.markers.len() - 1]])
// The open place's own pin (now last) does nothing; the other opens A.
let seq = card_seq
marker_tapped(1)
log.push(card_seq == seq)
mod.answers = [{status_code: 200 body: '{"elements":[]}'}]
marker_tapped(0)
log.push([place.name place.cat mod.urls.len()])
// No pin there, or no index at all: nothing.
seq = card_seq
let infinite = 1 / 0
let not_a_number = 0 / 0
for i in [-1 2 99 0.5 nil "0" infinite not_a_number true] { marker_tapped(i) }
log.push([card_seq == seq place.name])
// Directions and the drive show the route's pins, not these.
screen = "route"
marker_tapped(0)
log.push([card_seq == seq place.name])
log.to_json()"#;
    let out = maps_model(&format!("{CARD_STUBS}{code}"));
    assert_eq!(
        out,
        serde_json::json!([
            "37.3,-121.3,1;37.1,-121.1,1",
            ["place", {"id": "", "name": "Raw", "cat": "", "label": "", "lat": 37.3, "lon": -121.3},
             "Raw", "", 0, "37.1,-121.1,1;37.3,-121.3,2"],
            true,
            ["A", "Mall", 1],
            [true, "A"],
            [true, "A"]
        ])
    );
}

#[test]
fn maps_shows_a_places_distance_only_with_a_fix() {
    // The card's distance line: hidden without a fix (an empty line would
    // stay as a gap), straight-line text with one, and kept current by the
    // timer's tick.
    let code = r#"let santana = {id: "" name: "Santana Row" cat: "" label: "" lat: 37.3209796 lon: -121.9486002}
let log = []
open_place(santana)
log.push([mod.texts["pdist"] mod.shown["pdist"]])
mod.fix = true
tick()
log.push([mod.texts["pdist"] mod.shown["pdist"]])
log.to_json()"#;
    let out = maps_model(&format!("{CARD_STUBS}{code}"));
    assert_eq!(out, serde_json::json!([["", false], ["5.8 km away", true]]));
}

/// Photon's reverse lookup at two pressed points: a cafe a few meters from
/// the first, a street near the second.
const CAFE_HERE: &str = r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[-121.88512,37.33478]},"properties":{"osm_type":"N","osm_id":21,"osm_key":"amenity","osm_value":"cafe","name":"Corner Cafe","street":"Market Street","city":"San Jose","state":"California"}}]}"#;
const STREET_THERE: &str = r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[-121.94871,37.32093]},"properties":{"osm_type":"W","osm_id":22,"osm_key":"highway","osm_value":"residential","name":"Olin Avenue","city":"San Jose"}}]}"#;

#[test]
fn maps_shows_what_is_at_a_long_pressed_point_and_drops_an_answer_for_an_older_press() {
    let code = r#"let cafe = {status_code: 200 body: 'CAFE'}
let street = {status_code: 200 body: 'STREET'}
let hours = {status_code: 200 body: '{"elements":[{"tags":{"opening_hours":"24/7"}}]}'}
// The card's name, category and address, the place it is about, and the
// pins drawn last.
fn card(){ return [screen mod.texts["pname"] mod.texts["pcat"] mod.texts["paddr"] place mod.markers[mod.markers.len() - 1]] }
let log = []
// A press on the search screen: at once a "Dropped pin" card at the point,
// with its pin and no flight (the person pressed there); then Photon's
// name for the spot, still at the pressed point, and its details by its id.
mod.answers = [cafe hours]
mod.during = fn() { log.push(card()) }
map_long_pressed(37.3349, -121.8851)
log.push([card() detail.hours hosts() mod.flights])
// A second press before the first answer comes: only the second's shows,
// and the first asks Overpass nothing.
mod.urls = []
mod.answers = [cafe street]
mod.during = fn() { map_long_pressed(37.321, -121.9486) }
map_long_pressed(37.3349, -121.8851)
log.push([card() hosts()])
// Closed before the answer: the card stays closed, and nothing more is asked.
mod.urls = []
mod.answers = [cafe]
mod.during = fn() { close_place() }
map_long_pressed(37.3349, -121.8851)
log.push([screen place mod.urls.len() mod.markers[mod.markers.len() - 1]])
// Photon unreachable, an error page (HTTP or not), Photon naming nothing
// there, or a request the runtime refused: the card stays "Dropped pin" with
// the coordinates, and asks Overpass nothing.
mod.urls = []
let busy = {status_code: 503 body: 'CAFE'}
let page = {status_code: 200 body: '<html>busy</html>'}
let nothing = {status_code: 200 body: '{"features":[]}'}
let kept_pins = []
for res in [nil busy page nothing "refused"] {
    mod.answers = [res]
    map_long_pressed(37.3349, -121.8851)
    kept_pins.push([mod.texts["pname"] mod.texts["paddr"] place.id place.lat])
}
log.push([kept_pins mod.urls.len()])
// A press that isn't a place (MapView zoomed far out can report a longitude
// past 180) opens nothing and asks nothing.
mod.urls = []
let seq = card_seq
map_long_pressed(37.3, 181)
map_long_pressed(37.3, -200)
map_long_pressed(95, 1)
map_long_pressed(0, 0)
map_long_pressed(0 / 0, 1)
log.push([card_seq == seq mod.urls.len() place.name])
// Directions opened before the answer: its "To" names the place, and the
// route still goes to the pressed point (the cafe's details are kept).
mod.answers = [cafe]
mod.during = fn() { show("route") }
map_long_pressed(37.3349, -121.8851)
log.push([screen mod.texts["rto"] place.name place.lat place.lon mod.urls.len()])
// On Directions a long press does nothing.
seq = card_seq
map_long_pressed(37.5, -121.5)
log.push([card_seq == seq place.name mod.urls.len()])
log.to_json()"#
        .replace("CAFE", CAFE_HERE)
        .replace("STREET", STREET_THERE);
    let out = maps_model(&format!("{CARD_STUBS}{code}"));
    let here = "https://photon.komoot.io/reverse?lat=37.3349&lon=-121.8851&lang=en&limit=1";
    let there = "https://photon.komoot.io/reverse?lat=37.321&lon=-121.9486&lang=en&limit=1";
    let dropped = serde_json::json!({"id": "", "name": "Dropped pin", "cat": "", "label": "37.3349, -121.8851",
        "lat": 37.3349, "lon": -121.8851});
    let cafe = serde_json::json!({"id": "N:21", "name": "Corner Cafe", "cat": "Cafe",
        "label": "Market Street, San Jose, California", "lat": 37.3349, "lon": -121.8851});
    let street = serde_json::json!({"id": "W:22", "name": "Olin Avenue", "cat": "Street", "label": "San Jose",
        "lat": 37.321, "lon": -121.9486});
    let pin_here = "37.3349,-121.8851,2";
    let still_dropped = serde_json::json!(["Dropped pin", "37.3349, -121.8851", "", 37.3349]);
    assert_eq!(
        out,
        serde_json::json!([
            ["place", "Dropped pin", "", "37.3349, -121.8851", dropped, pin_here],
            [["place", "Corner Cafe", "Cafe", "Market Street, San Jose, California", cafe, pin_here],
             "24/7", [here, "https://overpass-api.de"], []],
            [["place", "Olin Avenue", "Street", "San Jose", street, "37.321,-121.9486,2"],
             [here, there, "https://overpass-api.de", "https://overpass.kumi.systems", "https://overpass.openstreetmap.fr"]],
            ["search", null, 1, ""],
            [[still_dropped, still_dropped, still_dropped, still_dropped, still_dropped], 5],
            [true, 0, "Dropped pin"],
            ["route", "Corner Cafe", "Corner Cafe", 37.3349, -121.8851, 1],
            [true, "Corner Cafe", 1]
        ])
    );
}
