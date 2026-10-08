//! Wasm Lab's real Rust functions (apps/wasmlab/guest, built into
//! apps/wasmlab/bundle/fns/wasmlab.wasm by its build.sh) in the runtime:
//! each gives exactly what the same code gives natively, and each way the
//! rogue function misbehaves ends in an error.

use std::time::{Duration, Instant};

use octosense_wasm_host::{CallError, Instance, Limits, Program, Runtime};
use serde_json::{json, Value};

const MODULE: &[u8] = include_bytes!("../../../apps/wasmlab/bundle/fns/wasmlab.wasm");

fn lab() -> (Runtime, Program) {
    let runtime = Runtime::new(
        Limits {
            deadline: Duration::from_millis(500),
            memory_bytes: 64 << 20,
            ..Limits::default()
        },
        None,
    )
    .unwrap();
    let program = runtime.load(MODULE).unwrap();
    (runtime, program)
}

fn guest() -> Instance {
    let (runtime, program) = lab();
    // The instance outlives the runtime, and keeps its deadline.
    runtime.instantiate(&program).unwrap()
}

fn call_json(guest: &mut Instance, name: &str, input: Value) -> Value {
    let out = guest
        .call(name, &serde_json::to_vec(&input).unwrap())
        .unwrap();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn the_module_offers_its_functions_and_imports_only_log() {
    assert_eq!(
        guest().functions(),
        [
            "find_slots",
            "fuzzy_rank",
            "md_to_html",
            "rogue",
            "text_diff"
        ]
    );
}

#[test]
fn markdown_matches_native() {
    let text = "# Notes\n\nSome *emphasis*, `code` and a [link](https://example.org).\n\n- [x] done\n- [ ] todo\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
    let wasm = guest().call("md_to_html", text.as_bytes()).unwrap();
    assert_eq!(
        wasm,
        wasmlab_functions::md_to_html(text.as_bytes()).unwrap()
    );
}

#[test]
fn slots_match_native() {
    let input = json!({"day_start": "08:30", "day_end": "18:00", "duration": 45, "step": 15,
        "busy": [["09:00", "10:00"], ["09:45", "11:15"], ["13:00", "14:00"], ["16:50", "17:05"]], "max": 20});
    let wasm = call_json(&mut guest(), "find_slots", input.clone());
    let native = wasmlab_functions::find_slots(serde_json::from_value(input).unwrap()).unwrap();
    assert_eq!(wasm, serde_json::to_value(native).unwrap());
    assert_eq!(wasm["slots"][0], json!(["11:15", "12:00"]));
}

#[test]
fn ranking_matches_native() {
    let items = [
        "Calendar",
        "Calculator",
        "Camera",
        "Local calls",
        "Mail",
        "Clock",
        "Weather",
        "Cal Newport notes",
    ];
    let input = json!({"query": "cal", "items": items, "limit": 5});
    let wasm = call_json(&mut guest(), "fuzzy_rank", input.clone());
    let native = wasmlab_functions::fuzzy_rank(serde_json::from_value(input).unwrap()).unwrap();
    assert_eq!(wasm, serde_json::to_value(native).unwrap());
}

#[test]
fn diff_matches_native() {
    let input = json!({"old": "one\ntwo\nthree\nfour\n", "new": "one\n2\nthree\nfour\nfive\n", "context": 1});
    let wasm = call_json(&mut guest(), "text_diff", input.clone());
    let native = wasmlab_functions::text_diff(serde_json::from_value(input).unwrap()).unwrap();
    assert_eq!(wasm, serde_json::to_value(native).unwrap());
}

#[test]
fn bad_input_is_the_functions_own_error() {
    let error = guest()
        .call("find_slots", b"{\"day_start\": 9}")
        .unwrap_err();
    assert!(
        matches!(&error, CallError::Guest(why) if why.contains("find_slots")),
        "{error:?}"
    );
    let error = guest()
        .call(
            "find_slots",
            br#"{"day_start": "18:00", "day_end": "09:00", "duration": 30}"#,
        )
        .unwrap_err();
    assert_eq!(
        error,
        CallError::Guest("the day ends before it starts".into())
    );
}

#[test]
fn rogue_code_ends_in_an_error_and_spends_only_its_own_instance() {
    let (runtime, program) = lab();
    let rogue = |mode: &str| {
        let mut guest = runtime.instantiate(&program).unwrap();
        let error = guest
            .call("rogue", json!({ "mode": mode }).to_string().as_bytes())
            .unwrap_err();
        assert!(guest.spent(), "{mode}");
        assert_eq!(
            guest.call("fuzzy_rank", br#"{"query": "m", "items": []}"#),
            Err(CallError::Spent)
        );
        (error, guest)
    };

    let started = Instant::now();
    assert_eq!(rogue("loop").0, CallError::Deadline);
    assert!(started.elapsed() < Duration::from_secs(2));

    let (error, guest) = rogue("alloc");
    assert!(
        matches!(&error, CallError::Trap(why) if why.starts_with("memory over its cap") && !why.contains("backtrace")),
        "{error:?}"
    );
    assert!(guest.memory_bytes() <= 64 << 20);

    let (error, mut guest) = rogue("panic");
    assert!(matches!(&error, CallError::Trap(_)), "{error:?}");
    let logs = guest.take_logs();
    assert!(
        logs.iter().any(|l| l.contains("rogue: a deliberate panic")),
        "{logs:?}"
    );

    let (error, _) = rogue("recurse");
    assert!(
        matches!(&error, CallError::Trap(why) if why.contains("stack")),
        "{error:?}"
    );

    // A fresh instance of the same program answers after all of it.
    let mut fresh = runtime.instantiate(&program).unwrap();
    let out = call_json(
        &mut fresh,
        "fuzzy_rank",
        json!({"query": "mail", "items": ["Mail", "Maps"]}),
    );
    assert_eq!(out["ranked"][0]["item"], "Mail");
}
