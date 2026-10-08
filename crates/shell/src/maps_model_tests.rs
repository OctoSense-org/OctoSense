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
