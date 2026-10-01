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

#[test]
fn the_title_follows_the_theme() {
    // Dark hosts draw News on a dark ground: a fixed dark ink disappeared.
    assert!(SCRIPT.contains(r#"Label{text: "News" draw_text.color: theme.color_text"#));
}
