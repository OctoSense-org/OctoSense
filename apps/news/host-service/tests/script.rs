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
fn a_failed_saved_story_write_keeps_the_previous_list_and_can_be_retried() {
    let code = format!(r#"
use mod.std.assert
let story = {{link: "local-story" title: "Local"}}
let saved = [story]
let save_error = ""
let deny = true
let disk = [story]
let ui_desk = false
let tab = "saved"
let notice = ""
let renders = 0
let fs = {{write: fn(path, data) {{
    if deny {{ assert(false) }}
    disk = data.parse_json()
}}}}
let ui = {{list: {{render: fn() {{ renders += 1 }} }}}}
fn data_path(name) {{ name }}
fn sync_save_notice() {{ notice = save_error }}
fn sync_reader_bar() {{}}
fn desk_saved_changed() {{}}
{}
{}
toggle_save(story)
let refused_remove = saved.len() == 1 && disk.len() == 1 && renders == 0 &&
    notice == "Couldn't remove this story from Saved. Try again."
deny = false
toggle_save(story)
let removed = saved.len() == 0 && disk.len() == 0 && notice == "" && renders == 1
deny = true
toggle_save(story)
let refused_save = saved.len() == 0 && disk.len() == 0 &&
    notice == "Couldn't save this story. Try again."
deny = false
toggle_save(story)
refused_remove && removed && refused_save && saved.len() == 1 && disk.len() == 1 && notice == ""
"#, function("is_saved"), function("toggle_save"));
    let (value, errors, _vm) = run(&code);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn failed_read_state_writes_leave_the_old_state_and_allow_retry() {
    let code = format!(r#"
use mod.std.assert
let desk_read = {{}}
let desk_read_links = []
let read_error = ""
let notice = ""
let disk = []
let deny = true
let fs = {{write: fn(path, data) {{
    if deny {{ assert(false) }}
    disk = data.parse_json()
}}}}
fn data_path(name) {{ name }}
fn sync_save_notice() {{ notice = read_error }}
{}
fn desk_is_read(link) {{ optional(desk_read, link, false) }}
{}
{}
{}
desk_mark_read("story")
let refused_read = !desk_is_read("story") && desk_read_links.len() == 0 && disk.len() == 0 && notice != ""
deny = false
desk_mark_read("story")
let stored_read = desk_is_read("story") && desk_read_links.len() == 1 && disk[0] == "story" && notice == ""
deny = true
desk_mark_unread("story")
let refused_unread = desk_is_read("story") && desk_read_links.len() == 1 && disk[0] == "story" && notice != ""
deny = false
desk_mark_unread("story")
refused_read && stored_read && refused_unread && !desk_is_read("story") && desk_read_links.len() == 0 && disk.len() == 0 && notice == ""
"#, function("optional"), function("desk_store_read"), function("desk_mark_read"), function("desk_mark_unread"));
    let (value, errors, _vm) = run(&code);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn read_state_limit_is_applied_only_after_a_successful_write() {
    let code = format!(r#"
use mod.std.assert
let desk_read = {{}}
let desk_read_links = []
let read_error = ""
let disk = []
let deny = true
let fs = {{write: fn(path, data) {{
    if deny {{ assert(false) }}
    disk = data.parse_json()
}}}}
fn data_path(name) {{ name }}
fn sync_save_notice() {{}}
{}
fn desk_is_read(link) {{ optional(desk_read, link, false) }}
{}
{}
let n = 0
while n < 600 {{
    let link = "story-" + n
    desk_read_links.push(link)
    desk_read[link] = true
    n += 1
}}
desk_mark_read("new")
let preserved = desk_read_links.len() == 600 && desk_read_links[0] == "story-0" && desk_is_read("story-0") && !desk_is_read("new")
deny = false
desk_mark_read("new")
preserved && desk_read_links.len() == 600 && disk.len() == 600 && disk[0] == "story-1" &&
    !desk_is_read("story-0") && desk_is_read("new")
"#, function("optional"), function("desk_store_read"), function("desk_mark_read"));
    let (value, errors, _vm) = run(&code);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn saved_and_read_errors_remain_visible_independently() {
    let code = format!(r#"
let save_error = "Save failed"
let read_error = "Read failed"
let ui_desk = true
let reading = {{link: "local"}}
let list_text = ""
let reader_text = ""
let list_visible = false
let reader_visible = false
let ui = {{
    saved_notice_list_text: {{set_text: fn(value) {{ list_text = value }} }}
    saved_notice_reader_text: {{set_text: fn(value) {{ reader_text = value }} }}
    saved_notice_list: {{set_visible: fn(value) {{ list_visible = value }} }}
    saved_notice_reader: {{set_visible: fn(value) {{ reader_visible = value }} }}
}}
fn desk_steps_aside() {{ false }}
{}
sync_save_notice()
let both = reader_visible && !list_visible && reader_text == "Save failed\nRead failed"
read_error = ""
sync_save_notice()
let save_remains = reader_visible && reader_text == "Save failed"
save_error = ""
read_error = "Read failed"
reading = nil
sync_save_notice()
both && save_remains && list_visible && !reader_visible && list_text == "Read failed"
"#, function("sync_save_notice"));
    let (value, errors, _vm) = run(&code);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn a_failed_cache_write_does_not_stop_service_results_or_their_completion() {
    let code = format!(r#"
use mod.std.assert
let sources = [{{id: "hn"}} {{id: "google"}}]
let rows_by = {{hn: [] google: []}}
let failed_by = {{hn: false google: false}}
let cache_errors = {{hn: false google: false}}
let deny = true
let service_waiting = 0
let listed = 0
let updated = 0
let use_service = true
let tab = "today"
let visible = false
let fs = {{write: fn(path, data) {{
    if deny && path == "cache_hn.json" {{ assert(false) }}
}}}}
let ui = {{cache_notice: {{set_visible: fn(value) {{ visible = value }} }}}}
let host = {{request: fn(method, args, reply) {{
    let data = {{items: [{{title: "local fixture"}}] feed_error: nil updated: 12}}.to_json().parse_json()
    reply({{is_ok: true data: data}})
}}}}
fn cache_path(name) {{ name }}
fn service_row(row) {{ row }}
fn listed_service() {{ listed += 1 }}
{}
{}
{}
{}
list_service()
sync_cache_notice()
let completed = service_waiting == 0 && listed == 1 && rows_by["hn"].len() == 1 && rows_by["google"].len() == 1 && updated == 12
let warned = cache_errors["hn"] && !cache_errors["google"] && visible
tab = "saved"
sync_cache_notice()
let hidden_for_saved = !visible
deny = false
write_cache("hn", rows_by["hn"])
tab = "today"
sync_cache_notice()
completed && warned && hidden_for_saved && !visible && !cache_errors["hn"]
"#, function("write_cache"), function("cache_has_error"), function("sync_cache_notice"), function("list_service"));
    let (value, errors, _vm) = run(&code);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(value.as_bool(), Some(true));
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
fn story_metadata_uses_singular_and_plural_counts() {
    let functions = format!("fn ago(_time){{ \"\" }}\n{}", function("meta"));
    for (points, comments, expected) in [
        ("1", "1", "Local  ·  1 point  ·  1 comment"),
        ("42", "0", "Local  ·  42 points  ·  0 comments"),
        ("nil", "nil", "Local"),
    ] {
        let code = format!(r#"{functions}
meta({{source: "Local" published: 0 points: {points} comments: {comments}}}) == "{expected}"
"#);
        let (value, errors, _vm) = run(&code);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(value.as_bool(), Some(true), "{points}, {comments}");
    }
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
    // The shared interface's style flags, which top-level lets read (a Mac's
    // window sizes), stand in as on a phone.
    let (_value, errors, _vm) = run(&format!("let ui_desk = false\nlet ui_unified = false\n{code}\nnil"));
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn a_late_preflight_failure_does_not_close_an_open_story() {
    let setup = r#"
let desk_ask = 5
let closes = 0
let renders = 0
let ui = {reader: {close: || {closes = closes + 1}} reader_state: {render: || {renders = renders + 1}}}
fn desk_remember(){}
"#;
    let browser_error = SCRIPT.lines().find(|line| line.starts_with("fn desk_page_error()"))
        .expect("the reader's error handler");
    for (state, action, expected, closed) in [
        ("loading", "desk_probe_failed(5)", "failed", 1),
        ("loading", "desk_probe_failed(4)", "loading", 0),
        ("page", "desk_probe_failed(5)", "page", 0),
        ("failed", "desk_probe_failed(5)", "failed", 0),
        ("page", "desk_page_error()", "failed", 1),
    ] {
        let code = format!(r#"{setup}
let desk_state = "{state}"
{}
{}
{browser_error}
{action}
desk_state == "{expected}" && closes == {closed} && renders == {closed}
"#, function("desk_page_failed"), function("desk_probe_failed"));
        let (value, errors, _vm) = run(&code);
        assert!(errors.is_empty(), "{state}, {action}: {errors:?}");
        assert_eq!(value.as_bool(), Some(true), "{state}, {action}");
    }
}

#[test]
fn touch_reader_retries_the_current_page_and_ignores_errors_after_close() {
    let setup = r#"
let reading = {link: "story" discussion: "comments"}
let touch_url = ""
let opened = ""
let closes = 0
let notice = false
let allow_open = true
fn touch_remember(){}
let ui = {
    reader: {
        close: || {closes = closes + 1}
        open: |url| {opened = url; allow_open}
    }
    reader_notice: {set_visible: |visible| {notice = visible}}
}
"#;
    let code = format!(r#"{setup}
{}
{}
{}
touch_load("story")
touch_load("comments")
touch_page_failed()
let failed = notice
touch_retry()
let retried = opened == "comments" && closes == 3 && !notice
allow_open = false
touch_load("refused")
let refused = notice
reading = nil
notice = false
touch_page_failed()
touch_retry()
failed && retried && refused && !notice && closes == 4
"#, function("touch_page_failed"), function("touch_load"), function("touch_retry"));
    let (value, errors, _vm) = run(&code);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn touch_reader_restores_its_page_and_list_after_a_restyle() {
    let code = format!(r#"
let tab = "saved"
let query = "ocean"
let reading = {{link: "story" discussion: "comments"}}
let touch_url = "comments"
{}
{}
{}
touch_remember()
tab = "today"
query = ""
reading = nil
touch_url = ""
touch_recall()
let restored = tab == "saved" && query == "ocean" && reading.link == "story" && touch_url == "comments"
reading = nil
touch_url = ""
touch_remember()
touch_recall()
restored && reading == nil && touch_url == ""
"#, function("optional"), function("touch_remember"), function("touch_recall"));
    let (value, errors, _vm) = run(&code);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn offline_feed_status_distinguishes_cached_stories_from_an_update() {
    let setup = r#"
let sources = [{id: "a"} {id: "b"} {id: "c"}]
let failed_by = {a: false b: false c: false}
let rows_by = {a: [1] b: [] c: []}
let tab = "today"
let query = ""
let busy = false
let service_fetching = false
let updated = 100
let ui_desk = false
let text_seen = "unset"
let visible = true
let ui = {status: {set_text: |text| {text_seen = text} set_visible: |shown| {visible = shown}}}
fn desk_sync_status(){}
fn ago(_time){ "now" }
fn ui_by(touch, _desk){ touch }
fn today_rows(){ rows_by.a }
"#;
    let functions = format!("{}\n{}\n{}\n{}", function("any_failed"), function("today_update_failure"),
        function("sync_status"), function("empty_text"));
    for (state, expected, shown) in [
        ("", "Updated just now", true),
        ("failed_by.a = true", "Some sources couldn't update. Tap Refresh to try again.", true),
        ("failed_by.a = true; failed_by.b = true; failed_by.c = true", "Couldn't update headlines. Tap Refresh to try again.", true),
        ("failed_by.a = true; failed_by.b = true; failed_by.c = true; rows_by.a = []", "", false),
        ("tab = \"a\"; failed_by.a = true", "Couldn't update this source. Tap Refresh to try again.", true),
        ("tab = \"b\"; failed_by.b = true", "", false),
        ("tab = \"saved\"; busy = true", "", false),
    ] {
        let code = format!(r#"{setup}
{functions}
{state}
sync_status()
text_seen == "{expected}" && visible == {shown}
"#);
        let (value, errors, _vm) = run(&code);
        assert!(errors.is_empty(), "{state}: {errors:?}");
        assert_eq!(value.as_bool(), Some(true), "{state}");
    }
    let code = format!(r#"{setup}
{functions}
failed_by.a = true
failed_by.b = true
failed_by.c = true
empty_text() == "Couldn't load your headlines. Tap Refresh to try again."
"#);
    let (value, errors, _vm) = run(&code);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn desktop_today_reports_partial_and_complete_feed_failures() {
    let setup = r#"
let sources = [{id: "a"} {id: "b"} {id: "c"}]
let failed_by = {a: false b: false c: false}
let tab = "today"
let query = ""
let busy = false
let service_fetching = false
let updated = 100
fn visible_rows(){ [1] }
fn desk_count(_shown){ "1 story" }
fn desk_when(_time){ "just now" }
"#;
    let functions = format!("{}\n{}\n{}", function("any_failed"), function("today_update_failure"), function("desk_status_text"));
    for (state, expected) in [
        ("", "Updated just now"),
        ("failed_by.a = true", "Some sources couldn't update"),
        ("failed_by.a = true; failed_by.b = true; failed_by.c = true", "Couldn't update headlines"),
        ("tab = \"saved\"; failed_by.a = true", "1 story"),
    ] {
        let code = format!("{setup}\n{functions}\n{state}\ndesk_status_text() == \"{expected}\"");
        let (value, errors, _vm) = run(&code);
        assert!(errors.is_empty(), "{state}: {errors:?}");
        assert_eq!(value.as_bool(), Some(true), "{state}");
    }
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
