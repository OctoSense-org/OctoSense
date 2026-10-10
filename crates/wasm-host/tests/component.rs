//! Components (ADR 0014): typed exports called with JSON, and the WASI 0.2
//! subset an app's component reaches. The fixtures are built from
//! `tests/component-guest` with plain `cargo build --target wasm32-wasip2`
//! (`build.sh` there).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use octosense_wasm_host::component::{is_component, Grants};
use octosense_wasm_host::{CallError, Limits, LoadError, Runtime};
use serde_json::json;

const NOTES: &[u8] = include_bytes!("fixtures/notes.component.wasm");
const NETPROBE: &[u8] = include_bytes!("fixtures/netprobe.component.wasm");

fn runtime() -> Runtime {
    Runtime::new(
        Limits {
            deadline: Duration::from_millis(500),
            memory_bytes: 64 << 20,
            ..Limits::default()
        },
        None,
    )
    .unwrap()
}

/// A fresh folder standing in for the app's storage.
fn storage() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "wasm-host-component-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_component_is_told_apart_from_a_module() {
    assert!(is_component(NOTES));
    assert!(!is_component(&wat::parse_str("(module)").unwrap()));
}

#[test]
fn exports_are_listed_with_their_wit_signatures() {
    let program = runtime().load_component(NOTES).unwrap();
    let names: Vec<&str> = program.exports().iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "analyze",
            "count",
            "echo-bytes",
            "grow",
            "now-ms",
            "random-u64",
            "read-file",
            "save-html",
            "spin",
            "to-html"
        ]
    );
    let analyze = &program.exports()[0];
    assert_eq!(
        analyze.params,
        [("markdown".to_string(), "string".to_string())]
    );
    assert_eq!(
        analyze.result.as_deref(),
        Some("record { words: u32, lines: u32, headings: list<string> }")
    );
}

#[test]
fn an_unmodified_crate_runs_and_records_come_back_as_objects() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    let html = instance
        .call_json("to-html", &json!({"markdown": "# Hi\n\nThere *you*"}))
        .unwrap();
    assert_eq!(html, json!("<h1>Hi</h1>\n<p>There <em>you</em></p>\n"));
    // A one-parameter function also takes the bare value.
    let stats = instance
        .call_json("analyze", &json!("# Title\n## Part\nsome words here"))
        .unwrap();
    assert_eq!(
        stats,
        json!({"words": 7, "lines": 3, "headings": ["Title", "Part"]})
    );
}

#[test]
fn state_persists_across_calls_on_one_instance() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    assert_eq!(instance.call_json("count", &json!(null)).unwrap(), json!(1));
    assert_eq!(instance.call_json("count", &json!({})).unwrap(), json!(2));
    let mut fresh = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    assert_eq!(fresh.call_json("count", &json!([])).unwrap(), json!(1));
}

#[test]
fn clocks_randomness_and_bytes_work() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    let now = instance.call_json("now-ms", &json!(null)).unwrap();
    assert!(now.as_u64().unwrap() > 1_700_000_000_000, "{now}");
    let a = instance.call_json("random-u64", &json!(null)).unwrap();
    let b = instance.call_json("random-u64", &json!(null)).unwrap();
    assert_ne!(a, b);
    // list<u8> travels as base64.
    let echoed = instance
        .call_json("echo-bytes", &json!("AAEC/w=="))
        .unwrap();
    assert_eq!(echoed, json!("AAEC/w=="));
    assert_eq!(
        instance
            .call_json("echo-bytes", &json!([0, 1, 2, 255]))
            .unwrap(),
        json!("AAEC/w==")
    );
}

#[test]
fn files_live_only_in_the_granted_storage_folder() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let dir = storage();
    let grants = Grants {
        storage_dir: Some(dir.clone()),
    };
    let mut instance = rt.instantiate_component(&program, &grants, None).unwrap();
    let written = instance
        .call_json(
            "save-html",
            &json!({"markdown": "# Saved", "path": "out.html"}),
        )
        .unwrap();
    assert_eq!(written, json!(15));
    assert_eq!(
        std::fs::read_to_string(dir.join("out.html")).unwrap(),
        "<h1>Saved</h1>\n"
    );
    assert_eq!(
        instance
            .call_json("read-file", &json!("/out.html"))
            .unwrap(),
        json!("<h1>Saved</h1>\n")
    );
    // Nothing outside the folder: escaping it fails inside the guest.
    for path in ["../../../../etc/hosts", "/../etc/hosts"] {
        match instance.call_json("read-file", &json!(path)) {
            Err(CallError::Guest(_)) => {}
            other => panic!("{path}: {other:?}"),
        }
    }
    // Without a storage grant there is no filesystem at all.
    let mut bare = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    assert!(matches!(
        bare.call_json("save-html", &json!({"markdown": "x", "path": "x.html"})),
        Err(CallError::Guest(_))
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_deadline_and_the_memory_cap_end_a_call_and_spend_the_instance() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    assert_eq!(
        instance.call_json("spin", &json!(null)),
        Err(CallError::Deadline)
    );
    assert!(instance.spent());
    assert_eq!(
        instance.call_json("count", &json!(null)),
        Err(CallError::Spent)
    );
    let mut other = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    match other.call_json("grow", &json!(128)) {
        Err(CallError::Trap(why)) => assert!(why.contains("memory"), "{why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn arguments_that_do_not_fit_are_refused_with_the_parameter_name() {
    let rt = runtime();
    let program = rt.load_component(NOTES).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    match instance.call_json("save-html", &json!({"markdown": 3, "path": "x"})) {
        Err(CallError::Guest(why)) => assert!(why.starts_with("markdown:"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        instance.call_json("nope", &json!(null)),
        Err(CallError::NoSuchFunction(_))
    ));
}

#[test]
fn a_component_that_imports_sockets_is_refused() {
    match runtime().load_component(NETPROBE) {
        Err(LoadError::Import(name)) => assert!(name.starts_with("wasi:sockets/"), "{name}"),
        Err(other) => panic!("{other:?}"),
        Ok(_) => panic!("loaded a component that imports sockets"),
    }
}
