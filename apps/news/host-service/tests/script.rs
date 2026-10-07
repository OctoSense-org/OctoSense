//! Pieces of News's main.splash, run in the script VM: a story as the
//! `news` service sends it becomes a row even when it leaves fields out.
//! (Reading a missing field is an error in the script, not nil: a story
//! without `image` once stopped the list at "Updating…".)
use makepad_script::*;

const SCRIPT: &str = include_str!("../../bundle/main.splash");

/// The source of `fn <name>(…){…}` in main.splash, up to its closing brace
/// at the start of a line.
fn function(name: &str) -> &'static str {
    let start = SCRIPT.find(&format!("\nfn {name}(")).unwrap_or_else(|| panic!("fn {name} in main.splash")) + 1;
    let len = SCRIPT[start..].find("\n}\n").expect("the function's end") + 2;
    &SCRIPT[start..start + len]
}

fn run(code: &str) -> (ScriptValue, Vec<String>, ScriptVm<'static>) {
    let host = Box::leak(Box::new(ScriptVmHost::new((), ())));
    let mut vm = ScriptVm { host, bx: Box::new(ScriptVmBase::new()) };
    vm.bx.captured_errors = Some(Vec::new());
    let value = vm.with_instruction_limit(500_000, |vm| vm.eval(ScriptMod { file: "news_script_test.splash".into(), code: format!("{code}\n;"), ..Default::default() }));
    let errors = vm.take_errors();
    (value, errors, vm)
}

fn row_field(story: &str, field: &str) -> (ScriptValue, Vec<String>) {
    let code = format!("{}\n{}\nlet row = service_row({story})\nrow.{field}", function("optional"), function("service_row"));
    let (value, errors, _vm) = run(&code);
    (value, errors)
}

const BARE: &str = r#"{id: "1" title: "T" url: "https://example.org/a" source: "Axios" feed: "google" lang: "en" published: 5 fetched: 6 summary: "S" topics: []}"#;
const FULL: &str = r#"{id: "2" title: "T" url: "https://example.org/b" source: "Hacker News" feed: "hn" lang: "en" published: 5 fetched: 6 summary: "" topics: [] image: "https://example.org/i.png" discussion: "https://news.ycombinator.com/item?id=2" points: 12 comments: 3}"#;

#[test]
fn a_story_without_optional_fields_becomes_a_row() {
    for field in ["image", "discussion"] {
        let (value, errors) = row_field(BARE, field);
        assert!(errors.is_empty(), "{field}: {errors:?}");
        assert!(value.is_string_like(), "{field}: {value:?}");
    }
    for field in ["points", "comments"] {
        let (value, errors) = row_field(BARE, field);
        assert!(errors.is_empty(), "{field}: {errors:?}");
        assert!(value.is_nil(), "{field}: {value:?}");
    }
    let (value, errors) = row_field(BARE, "link");
    assert!(errors.is_empty() && value.is_string_like(), "{errors:?}");
}

#[test]
fn a_story_from_json_without_optional_fields_becomes_a_row() {
    // The service's answer arrives as JSON data (string keys).
    let json = r#"'{"id": "1", "title": "T", "url": "https://example.org/a", "source": "Axios", "feed": "google", "lang": "en", "published": 5, "fetched": 6, "summary": "S", "topics": []}'.parse_json()"#;
    let (value, errors) = row_field(json, "image");
    assert!(errors.is_empty(), "{errors:?}");
    assert!(value.is_string_like(), "{value:?}");
    let (value, errors) = row_field(json, "points");
    assert!(errors.is_empty() && value.is_nil(), "{value:?} {errors:?}");
}

#[test]
fn a_story_with_every_field_keeps_them() {
    let (value, errors) = row_field(FULL, "points");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(value.as_number(), Some(12.0));
    let code = format!("{}\n{}\nlet row = service_row({FULL})\nrow.image == \"https://example.org/i.png\" && row.discussion != \"\"", function("optional"), function("service_row"));
    let (value, errors, _vm) = run(&code);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn the_old_direct_read_is_the_error_the_device_showed() {
    // Guards the test itself: a bare `it.image` read of a story without it
    // is an error in this VM.
    let (_value, errors, _vm) = run(&format!("let it = {BARE}\nlet image = \"\"\nif it.image != nil {{ image = it.image }}\nimage"));
    assert!(errors.iter().any(|e| e.contains("not found")), "{errors:?}");
}

/// A run where a source failed is retried on its own, sooner at first: 15 s,
/// 30 s, 1 min … at most 15 min (the device's first fetch ran before the
/// network was up, and nothing fetched again).
#[test]
fn a_failed_run_is_retried_with_a_growing_wait() {
    let code = format!("{}\nlet d = 15\nlet waits = []\nfor i in 8 {{ waits.push(d) d = next_retry_delay(d) }}\nwaits.to_json()", function("next_retry_delay"));
    let (value, errors, vm) = run(&code);
    assert!(errors.is_empty(), "{errors:?}");
    let json = vm.bx.heap.string_with(value, |_, s| s.to_string()).unwrap_or_default();
    assert_eq!(json, "[15,30,60,120,240,480,900,900]");
    for call in ["schedule_retry()\n}", "start_timeout(wait, || {", "if !any_failed() {"] {
        assert!(SCRIPT.contains(call), "{call}");
    }
    // Both ends of a run schedule it: the service's list and the own fetch.
    assert_eq!(SCRIPT.matches("    schedule_retry()\n").count(), 2);
    // The retry and the interval keep the service's back-off (`due`); only
    // the person's Refresh and News opening skip a failed source's.
    assert!(SCRIPT.contains("retry_armed = false\n        refresh_due()"));
    assert!(SCRIPT.contains("start_interval(900, || refresh_due())"));
    assert!(SCRIPT.contains("host.request(\"news.refresh\", {due: due}"));
    assert!(SCRIPT.contains("on_click: || refresh()"));
}

/// Every function of the script parses and defines (all of main.splash up to
/// its first call at the top level, `start_timeout(…)`).
#[test]
fn the_scripts_functions_parse() {
    let code = SCRIPT.split_once("// END shared app interface\n").expect("shared interface").1
        .split_once("\nstart_timeout(").expect("the boot call").0;
    let (_value, errors, _vm) = run(&format!("{code}\nnil"));
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn opening_during_the_hosts_fetch_follows_completion_without_forcing_a_second_fetch() {
    let setup = r#"
let clock = 100
fn time_now(){ clock }
let busy = false
let use_service = true
let service_relist = false
let service_follow_until = 0
let service_fetching = false
let requested_due = false
let reply = {is_ok: true data: {busy: true}}
fn sync_status(){}
fn list_service(){}
let host = {request: |method, args, callback| {requested_due = args.due; callback(reply)}}
"#;
    for (reply, elapsed, following) in [
        ("{is_ok: true data: {busy: true}}", 2, true),
        ("{is_ok: true data: {total: 30}}", 2, false),
        ("{is_ok: false}", 2, false),
        ("{is_ok: true data: {busy: true}}", 61, false),
    ] {
        let code = format!(r#"{setup}
{}
{}
refresh_service(false)
let initially_following = service_fetching && service_follow_until == 160
clock = 100 + {elapsed}
reply = {reply}
busy = false
refresh_service(true)
initially_following && requested_due && service_follow_until == 160 && service_fetching == {following}
"#, function("optional"), function("refresh_service"));
        let (value, errors, _vm) = run(&code);
        assert!(errors.is_empty(), "{reply}: {errors:?}");
        assert_eq!(value.as_bool(), Some(true), "{reply}, elapsed={elapsed}");
    }
}

#[test]
fn the_title_follows_the_theme() {
    // Dark hosts draw News on a dark ground: a fixed dark ink disappeared.
    assert!(SCRIPT.contains(r#"Label{text: "News" draw_text.color: theme.color_text"#));
}
