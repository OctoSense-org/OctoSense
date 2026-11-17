//! The host-owned provider search must not show an older asynchronous result.
use makepad_script::*;

#[test]
fn a_late_provider_search_cannot_replace_the_newer_results_or_error() {
    let source = include_str!("../src/sheets.rs")
        .split_once("const ADD_SHEET: &str =")
        .expect("add-sheet source")
        .1;
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("sheet function") + 1;
        let end = source[start..].find("\n}\n").expect("sheet function end") + 2;
        &source[start..start + end]
    };
    let code = format!(r#"
let families = []
let families_request = 0
let callbacks = []
let notice = ""
let renders = 0
let ui = {{
    status: {{set_text: fn(value) {{ notice = value }} set_visible: fn(value) {{}} }}
    fam_list: {{render: fn() {{ renders = renders + 1 }} }}
}}
let host = {{request: fn(method, args, callback) {{ callbacks.push(callback) }} }}
fn wizard_current() {{ true }}
fn remember_touch() {{}}
{}
{}
load_families("first")
load_families("second")
load_families("newest")
callbacks[2]({{is_ok: true data: [{{id: "newest"}}]}})
callbacks[0]({{is_ok: true data: [{{id: "old"}}]}})
callbacks[1]({{is_ok: false error: "old failure"}})
families.len() == 1 && families[0].id == "newest" && notice == "" && renders == 1
;"#, function("show"), function("load_families"));
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm { host: &mut host, bx: Box::new(ScriptVmBase::new()) };
    vm.bx.captured_errors = Some(Vec::new());
    let value = vm.with_instruction_limit(500_000, |vm| vm.eval(ScriptMod {
        file: "provider_search_order.splash".into(), code, ..Default::default()
    }));
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "provider search script errors: {errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn a_late_model_list_cannot_replace_the_newly_selected_family() {
    let source = include_str!("../src/sheets.rs")
        .split_once("const ADD_SHEET: &str =")
        .expect("add-sheet source")
        .1;
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("sheet function") + 1;
        let end = source[start..].find("\n}\n").expect("sheet function end") + 2;
        &source[start..start + end]
    };
    let code = format!(r#"
let fam = nil
let fetched = []
let custom = false
let model = nil
let route = nil
let tested = ""
let models = []
let family_routes = []
let model_requests = 0
let want_model = ""
let want_family = ""
let desk = false
let edit_opening = false
let callbacks = []
let notice = ""
let field = {{set_text: fn(value) {{ notice = value }} set_visible: fn(value) {{}} }}
let view = {{render: fn() {{}} }}
let ui = {{custom_box: field fetch_note: field key_note: field status: field fam_list: view footer: view}}
let host = {{request: fn(method, args, callback) {{ callbacks.push(callback) }} }}
fn wizard_current() {{ true }}
fn remember_touch() {{}}
fn key_note() {{ "" }}
fn select_model(value) {{ model = value }}
fn use_custom(value) {{}}
fn keep_route() {{}}
fn form_open() {{}}
{}
{}
choose_family({{id: "old"}})
choose_family({{id: "new"}})
callbacks[1]({{is_ok: true data: {{models: [{{id: "new-model" default: true}}] routes: []}}}})
callbacks[0]({{is_ok: true data: {{models: [{{id: "old-model" default: true}}] routes: []}}}})
fam.id == "new" && model.id == "new-model" && models[0].id == "new-model" && notice == ""
;"#, function("show"), function("choose_family"));
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm { host: &mut host, bx: Box::new(ScriptVmBase::new()) };
    vm.bx.captured_errors = Some(Vec::new());
    let value = vm.with_instruction_limit(500_000, |vm| vm.eval(ScriptMod {
        file: "provider_model_order.splash".into(), code, ..Default::default()
    }));
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "provider model script errors: {errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn touch_wizard_state_does_not_store_a_key_or_accept_an_old_run() {
    let source = include_str!("../src/sheets.rs")
        .split_once("const ADD_SHEET: &str =")
        .expect("add-sheet source")
        .1;
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("sheet function") + 1;
        let end = source[start..].find("\n}\n").expect("sheet function end") + 2;
        &source[start..start + end]
    };
    let code = format!(r#"
let desk = false
let wizard_run = 1
mod.provider_wizard_run = 1
let busy = false
let editing = false
let want_family = ""
let want_model = ""
let want_route = ""
let families = []
let fam = {{id: "family"}}
let models = []
let family_routes = []
let fetched = []
let model = {{id: "model"}}
let custom = false
let route = {{id: "official"}}
let protocol = "openai"
let step = 4
let tested = ""
let result = ""
let reason = ""
let result_ms = ""
let save_issue = ""
{}
{}
remember_touch()
let kept = mod.provider_touch_wizard
let no_key = try {{ kept.key == nil }} catch {{ true }}
let saved_step = kept.step == 4 && kept.fam.id == "family" && kept.model.id == "model"
mod.provider_wizard_run = 2
step = 5
remember_touch()
let stale_ignored = mod.provider_touch_wizard.step == 4
mod.provider_wizard_run = 1
tested = "testing"
remember_touch()
let pending_kept = mod.provider_touch_wizard.tested == "testing"
busy = true
remember_touch()
saved_step && no_key && stale_ignored && pending_kept && mod.provider_touch_wizard.busy == true
;"#, function("wizard_current"), function("remember_touch"));
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm { host: &mut host, bx: Box::new(ScriptVmBase::new()) };
    vm.bx.captured_errors = Some(Vec::new());
    let value = vm.with_instruction_limit(500_000, |vm| vm.eval(ScriptMod {
        file: "provider_touch_state.splash".into(), code, ..Default::default()
    }));
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "provider state script errors: {errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn a_pending_phone_save_delivers_its_error_to_the_current_wizard_once() {
    let source = include_str!("../src/sheets.rs")
        .split_once("const ADD_SHEET: &str =")
        .expect("add-sheet source")
        .1;
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("sheet function") + 1;
        let end = source[start..].find("\n}\n").expect("sheet function end") + 2;
        &source[start..start + end]
    };
    let code = format!(r#"
let desk = false
let active = true
let busy = false
let save_issue = ""
let fam = {{id: "family"}}
let notice = ""
let callbacks = []
let view = {{render: fn() {{}}}}
let ui = {{header_action: view footer: view status: {{}}}}
let host = {{request: fn(method, args, callback) {{ callbacks.push(callback) }} }}
fn wizard_current() {{ active }}
fn remember_touch() {{}}
fn form(check) {{ {{}} }}
fn show(label, text) {{ notice = text }}
fn start_timeout(delay, callback) {{}}
{}
{}
{}
{}
{}
{}
save()
let one_request = busy && callbacks.len() == 1
cancel()
let cancel_blocked = busy && callbacks.len() == 1
active = false
callbacks[0]({{is_ok: false error: "invalid: parse private-path: malformed JSON"}})
let held_for_new_run = busy && mod.provider_touch_save.response != nil
active = true
follow_touch_save(1)
let delivered = !busy && mod.provider_touch_save == nil && notice == "Couldn't save the model because the saved provider settings are unavailable. Try again."
save()
let retried_once = busy && callbacks.len() == 2
callbacks[0]({{is_ok: false error: "old failure"}})
let old_ignored = busy && mod.provider_touch_save.id == 2 && mod.provider_touch_save.response == nil
callbacks[1]({{is_ok: false error: "That provider is already in the list."}})
one_request && cancel_blocked && held_for_new_run && delivered && retried_once && old_ignored && !busy && notice == "That provider is already in the list."
;"#, function("save_message"), function("save_controls"), function("apply_save_result"), function("follow_touch_save"), function("save"), function("cancel"));
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm { host: &mut host, bx: Box::new(ScriptVmBase::new()) };
    vm.bx.captured_errors = Some(Vec::new());
    let value = vm.with_instruction_limit(500_000, |vm| vm.eval(ScriptMod {
        file: "provider_touch_save_result.splash".into(), code, ..Default::default()
    }));
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "provider save result script errors: {errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}

#[test]
fn a_phone_test_result_follows_the_current_wizard_without_repeating_the_request() {
    let source = include_str!("../src/sheets.rs")
        .split_once("const ADD_SHEET: &str =")
        .expect("add-sheet source")
        .1;
    let function = |name: &str| {
        let start = source.find(&format!("\nfn {name}(")).expect("sheet function") + 1;
        let end = source[start..].find("\n}\n").expect("sheet function end") + 2;
        &source[start..start + end]
    };
    let code = format!(r#"
let desk = false
let active = true
let busy = false
let tested = ""
let result = ""
let reason = ""
let result_ms = ""
let save_after_test = false
let callbacks = []
let view = {{render: fn() {{}}}}
let ui = {{test_box: view footer: view}}
let host = {{request: fn(method, args, callback) {{ callbacks.push(callback) }} }}
fn wizard_current() {{ active }}
fn remember_touch() {{}}
fn form(check) {{ {{}} }}
fn save() {{}}
fn start_timeout(delay, callback) {{}}
{}
{}
{}
{}
test()
let one_request = callbacks.len() == 1 && tested == "testing"
active = false
callbacks[0]({{is_ok: true data: {{ms: 83 error: nil}}}})
let held_for_new_run = tested == "testing" && mod.provider_touch_test.response.data.ms == 83
active = true
follow_touch_test(1)
let delivered_once = tested == "ok" && result_ms == "83" && mod.provider_touch_test == nil
tested = ""
test()
tested = ""
test()
callbacks[1]({{is_ok: false error: "old failure"}})
let old_ignored = tested == "testing" && mod.provider_touch_test.id == 3 && mod.provider_touch_test.response == nil
callbacks[2]({{is_ok: false error: "new failure"}})
let new_delivered = tested == "fail" && result == "new failure" && mod.provider_touch_test == nil
tested = ""
test()
cancel()
callbacks[3]({{is_ok: true data: {{ms: 4}}}})
one_request && held_for_new_run && delivered_once && old_ignored && new_delivered && mod.provider_touch_test == nil && tested == "testing"
;"#, function("apply_test_result"), function("follow_touch_test"), function("test"), function("cancel"));
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm { host: &mut host, bx: Box::new(ScriptVmBase::new()) };
    vm.bx.captured_errors = Some(Vec::new());
    let value = vm.with_instruction_limit(500_000, |vm| vm.eval(ScriptMod {
        file: "provider_touch_test_result.splash".into(), code, ..Default::default()
    }));
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "provider test result script errors: {errors:?}");
    assert_eq!(value.as_bool(), Some(true));
}
