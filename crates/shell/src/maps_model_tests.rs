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
// MapView would ignore a flight to a point that isn't a place.
fly(0, 0, 14)
[log mod.flights seen].to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([
            [false, [true, false], [false, true], false, null, false, "untouched"],
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
    browse_box: widget locate_box: widget drive_box: widget drive_bar: widget}
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
// No answer, an HTTP error, no body, not Photon's, or a request the runtime
// refused: search isn't available. Photon with no places: none found.
mod.answers = [nil busy bodiless page "refused" none]
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
                ["failed", "failed", "failed", "failed", "failed", ["done", 0]]
            ],
            "https://photon.komoot.io/api/?q=Pizza&limit=8&lang=en&lat=40.7128&lon=-74.006",
            "OctoSense-Maps/1.0",
            // An emptied box asks for nothing.
            12
        ])
    );
}
