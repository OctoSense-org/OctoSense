//! The touch app must refresh its current list after a host sheet or restyle.
use makepad_script::*;

#[test]
fn stale_provider_loads_are_ignored_and_sheet_refresh_uses_the_current_run() {
    let source = include_str!("../../bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("app function") + 1;
        let end = source[start..].find("\n}\n").expect("app function end") + 2;
        &source[start..start + end]
    };
    let code = format!(r#"
let ui_desk = false
let touch_run = 1
let load_request = 0
let providers = []
let scanner = false
let store = ""
let image_picker = false
let filter_there = false
let talk_there = false
let query = ""
let notice = ""
let rendered = ""
let callbacks = []
let refreshes = 0
let widget = {{set_visible: fn(value) {{}} set_text: fn(value) {{}}}}
let ui = {{add: widget talk: widget saved_heading: widget export_qr: widget scan: widget
          paste: widget where: widget filter_box: widget}}
let host = {{request: fn(method, args, callback) {{ callbacks.push(callback) }} }}
fn status(text) {{ notice = text }}
fn desk_list_failed() {{}}
fn desk_where() {{ "" }}
fn apply_filter() {{ if providers.len() > 0 {{ rendered = providers[0].id }} }}
{}
{}
{}
{}
start()
load()
callbacks[1]({{is_ok: true data: {{primary: {{id: "new"}} fallbacks: [] scanner: false store: "profile" image_picker: false}}}})
callbacks[0]({{is_ok: true data: {{primary: {{id: "old"}} fallbacks: [] scanner: false store: "profile" image_picker: false}}}})
let newer_wins = providers[0].id == "new" && rendered == "new"
load()
mod.ai_providers_touch_run = 2
callbacks[2]({{is_ok: false error: "stale failure"}})
let old_run_ignored = notice == "" && rendered == "new"
mod.ai_providers_touch_reload = || {{ refreshes = refreshes + 1 }}
refresh_current()
newer_wins && old_run_ignored && refreshes == 1
;"#, function("touch_current"), function("refresh_current"), function("load"), function("start"));
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm { host: &mut host, bx: Box::new(ScriptVmBase::new()) };
    vm.bx.captured_errors = Some(Vec::new());
    let value = vm.with_instruction_limit(500_000, |vm| vm.eval(ScriptMod {
        file: "provider_app_refresh.splash".into(), code, ..Default::default()
    }));
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "provider refresh script errors: {errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn touch_test_results_survive_a_new_run_and_ignore_a_removed_models_old_reply() {
    let source = include_str!("../../bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("app function") + 1;
        let end = source[start..].find("\n}\n").expect("app function end") + 2;
        &source[start..start + end]
    };
    let code = format!(r#"
let ui_desk = false
let results = []
let callbacks = []
let renders = 0
let ui = {{rows: {{render: fn() {{}}}}}}
let host = {{request: fn(method, args, callback) {{ callbacks.push(callback) }} }}
fn desk_refresh() {{}}
{}
{}
{}
{}
{}
{}
{}
{}
{}
mod.ai_providers_touch_render = || {{ renders = renders + 1 }}
run_test({{id: "model"}})
run_test({{id: "model"}})
let one_pending = callbacks.len() == 1 && state_of("model") == "testing"
callbacks[0]({{is_ok: true data: {{error: nil ms: 8}}}})
let delivered = state_of("model") == "ok" && result_of("model") == "Connected · 8 ms" && renders == 2
run_test({{id: "model"}})
discard_result("model")
run_test({{id: "model"}})
callbacks[1]({{is_ok: false error: "old result"}})
let old_ignored = state_of("model") == "testing" && result_request("model") == 3
callbacks[2]({{is_ok: true data: {{error: nil ms: 3}}}})
one_pending && delivered && old_ignored && state_of("model") == "ok" && result_of("model") == "Connected · 3 ms"
;"#, function("current_results"), function("result_of"), function("state_of"), function("detail_of"), function("result_request"),
        function("render_results"), function("set_result_detail"), function("discard_result"), function("run_test"));
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm { host: &mut host, bx: Box::new(ScriptVmBase::new()) };
    vm.bx.captured_errors = Some(Vec::new());
    let value = vm.with_instruction_limit(500_000, |vm| vm.eval(ScriptMod {
        file: "provider_touch_row_result.splash".into(), code, ..Default::default()
    }));
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "provider row test script errors: {errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}
