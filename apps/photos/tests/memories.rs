use makepad_script::*;
use octosense_llm_service::complete::{schema::Schema, Request};
use serde_json::{json, Value};

const SCRIPT: &str = include_str!("../bundle/main.splash");
const STORY: &str = r#"{title: "By the water" summary: "A few days by the coast." photos: ["coast" "beach" "family-four-beach"]}"#;

// Only the environment is substituted. All Photos functions below run in
// the pinned production VM, including asynchronous reply handling and storage.
const ENV: &str = r#"
use mod.std.assert
let files = {}
let writes_fail = false
let partial_writes = false
let fs = {
    exists: fn(path){ for k v in files { if k == path { return true } }; false }
    read: fn(path){ files[path] }
    write: fn(path, data){ assert(!writes_fail); if partial_writes { files[path] = "{broken"; assert(false) }; files[path] = data }
    remove: fn(path){ files.delete(path) }
}
let pending = []
let timers = []
let timeout_seconds = []
let cancelled_timers = []
fn start_timeout(seconds, callback){ timeout_seconds.push(seconds); timers.push(callback); timers.len() }
fn start_interval(seconds, callback){ timers.push(callback); timers.len() }
fn stop_timer(id){ cancelled_timers.push(id) }
let host = {request: fn(method, args, callback){ pending.push({method: method args: args callback: callback}); pending.len() }}
let widget = {render: fn(){} set_text: fn(text){} set_visible: fn(visible){} text: fn(){ "" }}
let ui = {title: widget subtitle: widget back: widget create: widget edit: widget searchbar: widget editor_bar: widget tabs: widget message: widget list: widget memory_controls: widget memory_status: widget viewer: widget main: widget viewer_title: widget viewer_meta: widget viewer_count: widget favorite: widget stage: widget playback: widget tile_grid: widget}
"#;

fn run(body: &str) -> Value {
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm { host: &mut host, bx: Box::new(ScriptVmBase::new()) };
    vm.bx.captured_errors = Some(Vec::new());
    let logic = SCRIPT.split_once("\nstart_timeout(").expect("boot boundary").0;
    let value = vm.with_instruction_limit(4_000_000, |vm| vm.eval(ScriptMod {
        file: "photos_memories_test.splash".into(),
        code: format!("{ENV}\n{logic}\n{body}\n;"),
        ..Default::default()
    }));
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "{errors:?}: Splash errors executing {body}");
    assert!(!value.is_err(), "Splash returned {value:?}");
    let text = vm.bx.heap.string_with(value, |_, s| s.to_string()).expect("JSON result");
    serde_json::from_str(&text).expect(&text)
}

#[test]
fn photos_bundle_is_admitted_by_the_shared_app_contract() {
    use octosense_app_contract::{admit_digest, digest_dir, parse, resolve, HostLimits, RefuseAllSignatures};

    let bundle = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../photos/bundle");
    let digest = digest_dir(&bundle).expect("digest the shipped Photos bundle");
    let mut manifest: Value = serde_json::from_str(include_str!("../bundle/manifest.json")).unwrap();
    // The shell stamps system-bundle digests at build time, before admission.
    manifest["integrity"]["bundle_blake3"] = json!(digest);
    let manifest = parse(&manifest.to_string()).expect("Photos conforms to the published app contract");
    admit_digest(&manifest, &digest, &RefuseAllSignatures).expect("admit the stamped bundle");
    let policy = resolve(&manifest, &HostLimits::system()).expect("resolve system-app permissions");
    assert_eq!(policy.app_id, "os.photos");
    assert!(policy.allows("model"));
    assert!(policy.allows("storage"));
    assert!(policy.hosts.is_empty(), "provider networking belongs to the host");
    assert!(!policy.storage.accounts, "Photos keeps its device-local library");
    assert!(manifest.agent.is_none(), "one-shot curation needs no app agent");
}

#[test]
fn existing_library_and_moments_work_without_ai() {
    let v = run("load_store()\nmemories = build_memories()\n{albums: store.albums.len() favorites: store.favorites.len() moments: memories.len() requests: pending.len()}.to_json()");
    assert_eq!(v["albums"], 2);
    assert_eq!(v["favorites"], 3);
    assert!(v["moments"].as_u64().unwrap() > 1);
    assert_eq!(v["requests"], 0);
}

#[test]
fn valid_stories_keep_photo_order_and_derive_dates_from_catalog() {
    let v = run(&format!("checked_memories([{STORY}], catalog_ids(), 3).to_json()"));
    assert_eq!(v[0]["title"], "By the water");
    assert_eq!(v[0]["photos"], json!(["coast", "beach", "family-four-beach"]));
    assert_eq!(v[0]["start"], "2026-07-12");
    assert_eq!(v[0]["end"], "2026-07-13");
}

#[test]
fn invalid_stories_are_rejected_without_script_errors() {
    for input in [
        "nil", "42", "{}", "[{}]", "[nil]",
        r#"[{title: "T" summary: "S" photos: ["beach" "invented"]}]"#,
        r#"[{title: "T" summary: "S" photos: ["beach" "beach"]}]"#,
        r#"[{title: " " summary: "S" photos: ["beach" "coast"]}]"#,
        r#"[{title: 3 summary: "S" photos: ["beach" "coast"]}]"#,
        r#"[{title: "T" summary: "S" photos: "beach"}]"#,
        r#"[{title: "T" summary: "S" photos: ["beach"]}]"#,
    ] {
        assert_eq!(run(&format!("checked_memories({input}, catalog_ids(), 3).to_json()")), json!([]), "{input}");
    }
    assert_eq!(run(&format!("checked_memories([{STORY}], [\"beach\"], 3).to_json()")), json!([]));
}

#[test]
fn generation_uses_the_real_host_contract_and_bounded_metadata() {
    let v = run("memory_prompt = \"summer with family\"\ngenerate_memories()\ngenerate_memories()\n{count: pending.len() method: pending[0].method args: pending[0].args}.to_json()");
    assert_eq!(v["count"], 1, "double-click cannot spend twice");
    assert_eq!(v["method"], "model.complete");
    let args = &v["args"];
    Request::from_args(args).expect("host accepts Photos' actual request");
    let schema = Schema::compile(&args["schema"]).expect("supported schema");
    schema.validate(&json!({"memories": [{"title":"T","summary":"S","photos":["beach","coast"]}]})).unwrap();
    assert_eq!(args["input"]["prompt"], "summer with family");
    assert!(args["input"].to_string().len() < 32 * 1024);
    assert!(!args.to_string().contains("{{assets}}"));
    for photo in args["input"]["photos"].as_array().unwrap() {
        assert!(photo.get("file").is_none(), "metadata only");
    }
}

#[test]
fn successful_generation_survives_reload_and_keeps_albums() {
    let v = run(&format!(r#"
        load_store()
        generate_memories()
        pending[0].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})
        saved_memories = []
        load_memories()
        {{saved: saved_memories albums: store.albums.len() busy: memory_busy}}.to_json()
    "#));
    assert_eq!(v["saved"][0]["title"], "By the water");
    assert_eq!(v["albums"], 2);
    assert_eq!(v["busy"], false);
}

#[test]
fn failures_and_cancellation_keep_saved_memories_and_ignore_late_replies() {
    for outcome in [
        r#"pending[0].callback({is_ok: false error: "no_provider: Configure AI providers."})"#.to_string(),
        "pending[0].callback({is_ok: true data: {}})".into(),
        "pending[0].callback({is_ok: true data: {output: {memories: []}}})".into(),
        format!("cancel_memories()\npending[0].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})"),
        format!("timers[0]()\npending[0].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})"),
    ] {
        let v = run(&format!("saved_memories = checked_memories([{STORY}], catalog_ids(), 12)\ngenerate_memories()\n{outcome}\n{{count: saved_memories.len() busy: memory_busy status: memory_status}}.to_json()"));
        assert_eq!(v["count"], 1, "{outcome}");
        assert_eq!(v["busy"], false, "{outcome}");
        assert!(!v["status"].as_str().unwrap().is_empty());
    }
}

#[test]
fn the_watchdog_allows_the_hosts_provider_attempts_to_finish() {
    use octosense_llm_service::complete::{ATTEMPTS, TIMEOUT};
    let seconds = run("generate_memories()\ntimeout_seconds[0].to_json()").as_u64().unwrap();
    assert!(std::time::Duration::from_secs(seconds) > TIMEOUT * ATTEMPTS,
        "Photos must leave time for the host's provider attempts and timeout margin");
}

#[test]
fn host_timeout_preserves_stories_and_allows_another_request() {
    let v = run(&format!(r#"
        saved_memories = checked_memories([{STORY}], catalog_ids(), 12)
        generate_memories()
        pending[0].callback({{is_ok: false error: "the host service timed out"}})
        let status = memory_status
        let stopped = !memory_busy && memory_timer == nil
        pending[0].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})
        let count = saved_memories.len()
        generate_memories()
        {{status: status stopped: stopped count: count requests: pending.len() busy: memory_busy}}.to_json()
    "#));
    assert_eq!(v["status"], "This is taking too long. Try again in a moment.");
    assert_eq!(v["stopped"], true);
    assert_eq!(v["count"], 1);
    assert_eq!(v["requests"], 2);
    assert_eq!(v["busy"], true);
}

#[test]
fn corrupt_storage_and_failed_writes_do_not_destroy_existing_stories() {
    for data in ["{broken", "{}", r#"{"version":99,"memories":[]}"#, r#"{"version":1,"memories":[{}]}"#] {
        let v = run(&format!("files[DATA + \"memories.json\"] = {}\nload_memories()\nsaved_memories.to_json()", serde_json::to_string(data).unwrap()));
        assert_eq!(v, json!([]));
    }
    let v = run(&format!("saved_memories = checked_memories([{STORY}], catalog_ids(), 12)\nwrites_fail = true\ngenerate_memories()\npending[0].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})\n{{count: saved_memories.len() busy: memory_busy status: memory_status}}.to_json()"));
    assert_eq!(v["count"], 1);
    assert_eq!(v["busy"], false);
    assert!(v["status"].as_str().unwrap().contains("save"));
}

#[test]
fn memory_navigation_and_slideshow_stop_cleanly() {
    let v = run(r#"
        load_memories()
        open_memory(0)
        let detail = route
        let ids = route_ids()
        play_story(route_arg)
        let started = playing
        viewer_index = viewer_ids.len() - 1
        advance_slides()
        let finished = !playing
        toggle_slides()
        let restarted = playing && viewer_index == 0
        close_viewer()
        go_back_fn()
        {detail: detail count: ids.len() started: started finished: finished restarted: restarted stopped: !playing timer: slide_timer route: route}.to_json()
    "#);
    assert_eq!(v["detail"], "memory");
    assert!(v["count"].as_u64().unwrap() >= 2);
    for key in ["started", "finished", "restarted", "stopped"] { assert_eq!(v[key], true, "{key}"); }
    assert_eq!(v["timer"], Value::Null);
    assert_eq!(v["route"], "memories");
}

#[test]
fn a_reply_from_an_earlier_request_cannot_finish_a_new_one() {
    let v = run(&format!(r#"
        generate_memories()
        cancel_memories()
        generate_memories()
        pending[0].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})
        let waiting = memory_busy && saved_memories.len() == 0
        pending[1].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})
        {{waiting: waiting count: saved_memories.len() busy: memory_busy}}.to_json()
    "#));
    assert_eq!(v, json!({"waiting": true, "count": 1, "busy": false}));
}

#[test]
fn memory_limits_bound_requests_and_saved_history() {
    assert_eq!(run("for i in 201 { memory_prompt = memory_prompt + \"a\" }\ngenerate_memories()\npending.len().to_json()"), 0);
    let v = run(&format!(r#"
        for i in 12 {{ saved_memories.push(checked_memories([{STORY}], catalog_ids(), 1)[0]) }}
        generate_memories()
        pending[0].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})
        saved_memories.len().to_json()
    "#));
    assert_eq!(v, 12);
}

#[test]
fn partial_writes_preserve_the_last_saved_snapshot_across_restarts() {
    let v = run(&format!(r#"
        generate_memories()
        pending[0].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})
        partial_writes = true
        generate_memories()
        pending[1].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})
        saved_memories = []
        load_memories()
        let first = saved_memories.len()
        partial_writes = false
        generate_memories()
        pending[2].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})
        partial_writes = true
        generate_memories()
        pending[3].callback({{is_ok: true data: {{output: {{memories: [{STORY}]}}}}}})
        saved_memories = []
        load_memories()
        {{first: first second: saved_memories.len()}}.to_json()
    "#));
    assert_eq!(v, json!({"first": 1, "second": 2}));
}
