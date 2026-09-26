//! Persistent, effect-free Octoscript application controller.
//!
//! A transition receives fresh data. Its state and effects are committed only
//! after the complete result and the host's finite requests have been checked.
//! This module knows no Settings pages, Android APIs, or widget event decisions.
use makepad_strict_json::Value;
use makepad_widgets::makepad_script::*;
use std::collections::HashSet;

const MAX_BYTES: usize = 512 * 1024;
const MAX_NODES: usize = 24_000;
const MAX_DEPTH: usize = 32;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

trait Session {
    fn with_vm<T>(&mut self, operation: impl FnOnce(&mut ScriptVm) -> T) -> T;
}
impl Session for ScriptVmHost {
    fn with_vm<T>(&mut self, operation: impl FnOnce(&mut ScriptVm) -> T) -> T {
        let bx = self.script_vm.take().unwrap_or_else(|| Box::new(ScriptVmBase::new()));
        let mut vm = ScriptVm { host: self, bx };
        let result = operation(&mut vm);
        let bx = vm.bx;
        self.script_vm = Some(bx);
        result
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Patch {
    Text(String, String),
    Input(String, String),
    Enabled(String, bool),
    Visible(String, bool),
    Scroll(String, f64, f64),
    Focus(String),
    Blur,
    Semantic(String, String),
    Label(String, String),
    Color(String, bool, [f32; 4]),
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub state: Value,
    pub patches: Vec<Patch>,
    pub requests: Vec<Value>,
    pub handled: bool,
}

/// Widget IDs come from the mounted bundled layout, never from backend data.
pub struct Controller {
    source: String,
    vm: Option<ScriptVmHost>,
    functions: Option<ScriptObjectRef>,
    state: Value,
    widgets: HashSet<String>,
}

pub fn bundled_source() -> String {
    [include_str!("../resources/settings/controller/core.octoscript"),
        include_str!("../resources/settings/controller/navigation.octoscript"),
        include_str!("../resources/settings/controller/basics.octoscript"),
        include_str!("../resources/settings/controller/app_policy.octoscript"),
        include_str!("../resources/settings/controller/apps.octoscript"),
        include_str!("../resources/settings/controller/updates.octoscript"),
        include_str!("../resources/settings/controller/network.octoscript"),
        include_str!("../resources/settings/controller/display.octoscript"),
        include_str!("../resources/settings/controller/wifi.octoscript"),
        include_str!("../resources/settings/controller/bluetooth.octoscript"),
        include_str!("../resources/settings/controller/datetime.octoscript"),
        include_str!("../resources/settings/controller/media.octoscript"),
        include_str!("../resources/settings/controller/app_notifications.octoscript"),
        include_str!("../resources/settings/controller/accounts.octoscript"),
        include_str!("../resources/settings/controller/dnd.octoscript"),
        include_str!("../resources/settings/controller/languages.octoscript"),
        include_str!("../resources/settings/controller/system_languages.octoscript"),
        include_str!("../resources/settings/controller/keyboards.octoscript"),
        include_str!("../resources/settings/controller/consent.octoscript"),
        include_str!("../resources/settings/controller/controls.octoscript"),
        include_str!("../resources/settings/controller/caption.octoscript"),
        include_str!("../resources/settings/controller/subscriptions.octoscript"),
        include_str!("../resources/settings/controller/accessibility.octoscript")].join("\n")
}

pub fn bundled_widgets() -> HashSet<String> {
    let tree = octoscript_makepad::design::prepare(include_str!("../resources/settings/settings.splash"))
        .expect("bundled Settings layout must evaluate");
    let mut ids = HashSet::new();
    let mut pending = vec![&tree];
    while let Some(node) = pending.pop() {
        if let Some(id) = &node.attrs.id { ids.insert(id.clone()); }
        pending.extend(&node.children);
    }
    ids
}

fn error(message: &str) -> String { format!("Settings controller: {message}") }

struct DataBudget { nodes: usize, bytes: usize }
impl DataBudget {
    fn new() -> Self { Self { nodes: MAX_NODES, bytes: MAX_BYTES } }
    fn charge(&mut self, depth: usize, bytes: usize) -> Result<(), String> {
        if depth > MAX_DEPTH || self.nodes == 0 || bytes > self.bytes {
            return Err(error("data limit exceeded"));
        }
        self.nodes -= 1;
        self.bytes -= bytes;
        Ok(())
    }
}

fn check_data(value: &Value, depth: usize, budget: &mut DataBudget) -> Result<(), String> {
    budget.charge(depth, 8)?;
    match value {
        Value::Int(n) if !(-MAX_SAFE..=MAX_SAFE).contains(n) => return Err(error("encode wide integers as decimal strings")),
        Value::F64(n) if !n.is_finite() || n.abs() > MAX_SAFE as f64 => return Err(error("non-finite or unsafe number")),
        Value::Str(text) => budget.charge(depth, text.len())?,
        Value::Arr(values) => for value in values { check_data(value, depth + 1, budget)?; },
        Value::Obj(values) => {
            let mut keys = HashSet::new();
            for (key, value) in values {
                budget.charge(depth, key.len())?;
                if !keys.insert(key) { return Err(error("duplicate data key")); }
                check_data(value, depth + 1, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Data is allocated directly, never interpolated into executable source.
fn import(heap: &mut ScriptHeap, value: &Value) -> ScriptValue {
    match value {
        Value::Null => NIL,
        Value::Bool(value) => (*value).into(),
        Value::Int(value) => ScriptValue::from_f64(*value as f64),
        Value::F64(value) => ScriptValue::from_f64(*value),
        Value::Str(value) => heap.new_string_from_str(value),
        Value::Arr(values) => {
            let array = heap.new_array();
            for value in values {
                let value = import(heap, value);
                heap.array_push_unchecked(array, value);
            }
            array.into()
        }
        Value::Obj(values) => {
            let object = heap.new_object();
            heap.set_string_keys(object);
            for (key, value) in values {
                let key = heap.new_string_from_str(key);
                let value = import(heap, value);
                heap.set_value_def(object, key, value);
            }
            object.into()
        }
    }
}

/// Only plain, finite data may leave the controller. Cycles, inherited objects,
/// functions and duplicate keys fail the whole transition rather than truncating.
fn export(heap: &ScriptHeap, value: ScriptValue, depth: usize, budget: &mut DataBudget, path: &mut Vec<ScriptValue>) -> Result<Value, String> {
    budget.charge(depth, 8)?;
    if value.is_nil() { return Ok(Value::Null); }
    if let Some(value) = value.as_bool() { return Ok(Value::Bool(value)); }
    if let Some(value) = value.as_number() {
        if !value.is_finite() || value.abs() > MAX_SAFE as f64 { return Err(error("non-finite or unsafe result number")); }
        return Ok(if value.fract() == 0.0 { Value::Int(value as i64) } else { Value::F64(value) });
    }
    if let Some(text) = heap.string_with(value, |_, text| text.to_owned()) {
        budget.charge(depth, text.len())?;
        return Ok(Value::Str(text));
    }
    if path.contains(&value) { return Err(error("cyclic result")); }
    path.push(value);
    let result = if let Some(array) = value.as_array() {
        let mut out = Vec::new();
        for index in 0..heap.array_len(array) {
            out.push(export(heap, heap.array_index_unchecked(array, index), depth + 1, budget, path)?);
        }
        Value::Arr(out)
    } else if let Some(object) = value.as_object() {
        if heap.proto(object) != id!(object).into() { return Err(error("result must contain plain records")); }
        let mut keys = HashSet::new();
        let mut out = Vec::new();
        for index in 0..heap.iter_len(object) {
            let pair = heap.iter_key_value(object, index, NoTrap);
            let key = heap.string_with(pair.key, |_, s| s.to_owned())
                .or_else(|| pair.key.as_id().and_then(|id| id.as_string(|s| s.map(str::to_owned))))
                .ok_or_else(|| error("non-text record key"))?;
            budget.charge(depth, key.len())?;
            if !keys.insert(key.clone()) { return Err(error("duplicate result key")); }
            out.push((key, export(heap, pair.value, depth + 1, budget, path)?));
        }
        Value::Obj(out)
    } else { return Err(error(&format!("non-data result type {:?}", value.value_type()))); };
    path.pop();
    Ok(result)
}

fn install_text(vm: &mut ScriptVm) {
    let text = vm.bx.heap.new_object();
    vm.add_method(text, id_lut!(lower), script_args_def!(value = NIL), |vm, args| {
        let value = script_value!(vm, args.value);
        let Some(value) = vm.bx.heap.string_with(value, |_, s| s.to_lowercase()) else { return NIL; };
        vm.bx.heap.new_string_from_str(&value)
    });
    vm.add_method(text, id_lut!(is_alphanumeric), script_args_def!(codepoint = NIL), |vm, args| {
        let value = script_value!(vm, args.codepoint).as_number();
        value.and_then(|n| (n.is_finite() && n.fract() == 0.0 && (0.0..=0x10ffff as f64).contains(&n)).then_some(n as u32))
            .and_then(char::from_u32).is_some_and(char::is_alphanumeric).into()
    });
    vm.add_method(text, id_lut!(from_codepoint), script_args_def!(codepoint = NIL), |vm, args| {
        let value = script_value!(vm, args.codepoint).as_number();
        let Some(value) = value.and_then(|n| (n.is_finite() && n.fract() == 0.0 && (0.0..=0x10ffff as f64).contains(&n)).then_some(n as u32)).and_then(char::from_u32) else { return NIL; };
        vm.bx.heap.new_string_from_str(&value.to_string())
    });
    vm.bx.heap.freeze(text);
    vm.set_injected_global(id!(text), text.into());
}

fn call(vm: &mut ScriptVm, function: ScriptValue, args: &[ScriptValue]) -> Result<Value, String> {
    let errors = vm.bx.uncaught_error_count;
    vm.bx.captured_errors = Some(Vec::new());
    // This effect-free VM admits only bounded data and local text operations.
    // Count work, not descheduled time: CPU contention must not discard input.
    // The instruction, stack, frame, heap and data limits still fail closed.
    let value = vm.with_stack_value_limit(16_384, |vm| vm.with_call_frame_limit(256, |vm| {
        vm.with_instruction_limit(1_000_000, |vm| vm.call(function, args))
    }));
    vm.drain_errors();
    if value.is_err() || vm.bx.uncaught_error_count != errors || vm.bx.threads.cur_ref().is_paused() {
        #[cfg(test)]
        eprintln!("Controller test diagnostics: {:?}", vm.take_errors().into_iter().take(3).collect::<Vec<_>>());
        return Err(error("transition failed; no effects applied"));
    }
    export(&vm.bx.heap, value, 0, &mut DataBudget::new(), &mut Vec::new())
}

impl Controller {
    pub fn new(source: &str, widgets: HashSet<String>) -> Result<Self, String> {
        if source.len() > MAX_BYTES { return Err(error("source limit exceeded")); }
        let mut out = Self { source: source.into(), vm: None, functions: None, state: Value::Null, widgets };
        out.start()?;
        let functions = out.functions.as_ref().unwrap().as_object();
        let state = out.vm.as_mut().unwrap().with_vm(|vm| {
            let function = vm.bx.heap.value(functions, id!(init).into(), NoTrap);
            call(vm, function, &[])
        })?;
        if !matches!(state, Value::Obj(_)) { return Err(error("initial state must be a record")); }
        out.state = state;
        Ok(out)
    }

    fn start(&mut self) -> Result<(), String> {
        let mut host = ScriptVmHost::new((), ());
        let functions = host.with_vm(|vm| {
            vm.bx.silence_errors = true;
            vm.bx.allow_debug_output = false;
            vm.bx.captured_errors = Some(Vec::new());
            vm.bx.heap.set_max_heap_bytes(Some(24 * 1024 * 1024));
            vm.bx.heap.set_max_string_bytes(Some(MAX_BYTES));
            // No Android, widgets, filesystem, network, shell, or tool module is
            // installed. Remove inherited debug entry points as well.
            let std = vm.bx.heap.module(id!(std));
            for member in [id!(log), id!(print), id!(println), id!(set_type_default)] {
                vm.bx.heap.set_value_def(std, member.into(), NIL);
            }
            install_text(vm);
            let compiled = vm.eval_checked(ScriptMod {
                cargo_manifest_path: String::new(), module_path: "octosense.settings".into(),
                file: "settings-controller.octoscript".into(), line: 0, column: 0,
                code: format!("{}\n{{init: settings_new, step: settings_step}}", self.source), values: Vec::new(),
            }, 2_000_000);
            let diagnostics = vm.take_errors();
            let value = compiled.ok_or_else(|| error(&format!("bundled source failed to compile: {}", diagnostics.into_iter().take(3).collect::<Vec<_>>().join("; "))))?;
            let object = value.as_object().ok_or_else(|| error("missing controller exports"))?;
            Ok::<_, String>(vm.bx.heap.new_object_ref(object))
        })?;
        self.functions = Some(functions);
        self.vm = Some(host);
        Ok(())
    }

    pub fn state(&self) -> &Value { &self.state }

    /// The caller validates every request against its finite typed vocabulary
    /// and current authority before any state/patch/request becomes visible.
    pub fn step<T>(&mut self, event: &Value, observed: &Value, validate: impl FnOnce(&Frame) -> Result<T, String>) -> Result<(Frame, T), String> {
        check_data(event, 0, &mut DataBudget::new())?;
        check_data(observed, 0, &mut DataBudget::new())?;
        if self.vm.is_none() { self.start()?; }
        let functions = self.functions.as_ref().unwrap().as_object();
        let result = self.vm.as_mut().unwrap().with_vm(|vm| {
            let function = vm.bx.heap.value(functions, id!(step).into(), NoTrap);
            let args = [import(&mut vm.bx.heap, &self.state), import(&mut vm.bx.heap, event), import(&mut vm.bx.heap, observed)];
            call(vm, function, &args)
        }).and_then(|value| Frame::decode(value, &self.widgets)).and_then(|frame| {
            let validated = validate(&frame)?;
            Ok((frame, validated))
        });
        match result {
            Ok((frame, validated)) => {
                self.state = frame.state.clone();
                self.vm.as_mut().unwrap().with_vm(|vm| { vm.bx.heap.mark(&vm.bx.threads, &vm.bx.code); vm.bx.heap.sweep(false); });
                Ok((frame, validated))
            }
            Err(failure) => {
                // Discard captured closure state as well as the mutated input.
                // Keep the last committed app state and never retry the event.
                self.functions = None;
                self.vm = None;
                Err(failure)
            }
        }
    }
}

fn fields(value: &Value, required: &[&str]) -> Result<(), String> {
    let Value::Obj(values) = value else { return Err(error("record required")); };
    if values.len() != required.len() || required.iter().any(|key| value.get(key).is_none()) {
        return Err(error("unexpected or missing fields"));
    }
    Ok(())
}
fn string(value: &Value, key: &str) -> Result<String, String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned).ok_or_else(|| error("text field required"))
}
fn boolean(value: &Value, key: &str) -> Result<bool, String> {
    value.get(key).and_then(Value::as_bool).ok_or_else(|| error("boolean field required"))
}
fn number(value: &Value, key: &str) -> Result<f64, String> {
    let value = match value.get(key) { Some(Value::Int(n)) => *n as f64, Some(Value::F64(n)) => *n, _ => return Err(error("numeric field required")) };
    if !value.is_finite() || !(0.0..=1_000_000.0).contains(&value) { return Err(error("invalid scroll coordinate")); }
    Ok(value)
}
impl Frame {
    fn decode(value: Value, widgets: &HashSet<String>) -> Result<Self, String> {
        fields(&value, &["state", "patch", "requests", "handled"])?;
        let state = value.get("state").filter(|v| matches!(v, Value::Obj(_))).ok_or_else(|| error("state record required"))?.clone();
        let raw_patches = value.get("patch").and_then(Value::as_arr).filter(|a| a.len() <= 4096).ok_or_else(|| error("bounded patch list required"))?;
        let requests = value.get("requests").and_then(Value::as_arr).filter(|a| a.len() <= 16).ok_or_else(|| error("bounded request list required"))?.to_vec();
        if requests.iter().any(|r| !matches!(r, Value::Obj(_)) || r.get("kind").and_then(Value::as_str).is_none()) { return Err(error("typed request record required")); }
        let mut patches = Vec::new();
        for patch in raw_patches {
            let op = string(patch, "op")?;
            if op == "blur" { fields(patch, &["op"])?; patches.push(Patch::Blur); continue; }
            let id = string(patch, "id")?;
            if !widgets.contains(&id) { return Err(error("patch target is not in bundled layout")); }
            patches.push(match op.as_str() {
                "text" | "input" | "semantic" | "label" => {
                    fields(patch, &["op", "id", "value"])?;
                    let text = string(patch, "value")?;
                    match op.as_str() { "text" => Patch::Text(id, text), "input" => Patch::Input(id, text), "semantic" => Patch::Semantic(id, text), _ => Patch::Label(id, text) }
                }
                "enabled" | "visible" => {
                    fields(patch, &["op", "id", "value"])?;
                    let flag = boolean(patch, "value")?;
                    if op == "enabled" { Patch::Enabled(id, flag) } else { Patch::Visible(id, flag) }
                }
                "color" => {
                    fields(patch, &["op", "id", "slot", "value"])?;
                    let foreground = match string(patch,"slot")?.as_str() {"text"=>true,"background"=>false,_=>return Err(error("invalid color slot"))};
                    let channels=patch.get("value").and_then(Value::as_arr).filter(|v|v.len()==4).ok_or_else(||error("RGBA required"))?;
                    let mut rgba=[0.0;4];
                    for (i,v) in channels.iter().enumerate() {let n=match v {Value::Int(n)=>*n as f64,Value::F64(n)=>*n,_=>return Err(error("numeric color required"))};if !n.is_finite() || !(0.0..=1.0).contains(&n){return Err(error(&format!("invalid color channel {n} for {id}")))}rgba[i]=n as f32;}
                    Patch::Color(id,foreground,rgba)
                }
                "scroll" => { fields(patch, &["op", "id", "x", "y"])?; Patch::Scroll(id, number(patch, "x")?, number(patch, "y")?) }
                "focus" => { fields(patch, &["op", "id"])?; Patch::Focus(id) }
                _ => return Err(error("unknown patch operation")),
            });
        }
        Ok(Self { state, patches, requests, handled: boolean(&value, "handled")? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_strict_json::{obj, s};
    fn event(kind: &str, value: Value) -> Value { obj(vec![("kind", s(kind)), ("value", value)]) }
    fn controller(body: &str) -> Controller {
        Controller::new(&format!("fn settings_new(){{return {{count:0}}}}\nfn settings_step(state,event,observed){{{body}}}"), ["title".into()].into_iter().collect()).unwrap()
    }
    const RETURN: &str = "return {state:state,patch:[],requests:[],handled:true}";
    #[test] fn script_color_patches_are_bounded_data_and_cannot_commit_partial_state() {
        for color in ["[1,0,0,2]", "[1,0,0]", "[1,0,0,undefined_call()]"] {
            let mut c=controller(&format!("state.count=1; return {{state:state,patch:[{{op:\"color\",id:\"title\",slot:\"background\",value:{color}}}],requests:[],handled:true}}"));
            let before=c.state().clone();
            assert!(c.step(&event("observe",Value::Null),&Value::Null,|_|Ok(())).is_err());assert_eq!(c.state(),&before);
        }
    }
    #[test] fn script_computes_state_and_unicode_without_source_injection() {
        let mut controller = controller(&format!("state.count = state.count + 1\nstate.value = text.lower(event.value)\n{RETURN}"));
        let input = "中文 😀 İ X\"; undefined_call() //\n\0";
        let (frame, ()) = controller.step(&event("input", s(input)), &Value::Null, |_| Ok(())).unwrap();
        assert_eq!(frame.state.get("value").and_then(Value::as_str), Some(input.to_lowercase().as_str()));
        assert_eq!(controller.state().get("count"), Some(&Value::Int(1)));
        controller.step(&event("input", s("NEXT")), &Value::Null, |_| Ok(())).unwrap();
        assert_eq!(controller.state().get("count"), Some(&Value::Int(2)));
    }
    #[test] fn failed_or_rejected_effect_cannot_commit_mutated_script_state() {
        let mut controller = controller("state.count = state.count + 1\nif event.value == true {undefined_call()}\nreturn {state:state,patch:[],requests:[{kind:\"brightness\",value:0.5,automatic:false}],handled:true}");
        assert!(controller.step(&event("click", Value::Bool(true)), &Value::Null, |_| Ok(())).is_err());
        assert_eq!(controller.state().get("count"), Some(&Value::Int(0)));
        assert!(controller.step(&event("click", Value::Bool(false)), &Value::Null, |_| Err::<(), _>("denied".into())).is_err());
        assert_eq!(controller.state().get("count"), Some(&Value::Int(0)));
        controller.step(&event("click", Value::Bool(false)), &Value::Null, |_| Ok(())).unwrap();
        assert_eq!(controller.state().get("count"), Some(&Value::Int(1)));
    }
    #[test] fn scheduler_delay_does_not_discard_a_bounded_transition() {
        let mut controller = controller(&format!("test.deschedule()\nstate.count=state.count+1\n{RETURN}"));
        // A test-only host function models time spent off CPU. No such blocking
        // operation is exposed to the bundled controller in production.
        controller.vm.as_mut().unwrap().with_vm(|vm| {
            let test = vm.bx.heap.new_object();
            vm.add_method(test, id_lut!(deschedule), script_args_def!(), |_, _| {
                std::thread::sleep(std::time::Duration::from_millis(120));
                NIL
            });
            vm.set_injected_global(id!(test), test.into());
        });
        controller.step(&event("click", Value::Null), &Value::Null, |_| Ok(())).unwrap();
        assert_eq!(controller.state().get("count"), Some(&Value::Int(1)));
    }
    #[test] fn incomplete_patches_and_cycles_fail_without_commit() {
        for body in [
            "state.count=1\nreturn {state:state,patch:[{op:\"text\",id:\"outside\",value:\"bad\"}],requests:[],handled:true}",
            "state.cycle=state\nreturn {state:state,patch:[],requests:[],handled:true}",
            "state.count=1\nreturn {state:state,patch:[{op:\"enabled\",id:\"title\",value:\"yes\"}],requests:[],handled:true}",
        ] {
            let mut controller = controller(body);
            assert!(controller.step(&event("click", Value::Null), &Value::Null, |_| Ok(())).is_err());
            assert_eq!(controller.state().get("count"), Some(&Value::Int(0)));
        }
    }
    #[test] fn infinite_controller_is_bounded_and_wide_identifiers_are_lossless_strings() {
        let mut controller = controller(&format!("if event.value == true {{while true {{}}}}\nstate.identifier=observed\n{RETURN}"));
        assert!(controller.step(&event("click", Value::Bool(true)), &Value::Null, |_| Ok(())).is_err());
        assert!(controller.step(&event("click", Value::Bool(false)), &Value::Int(i64::MAX), |_| Ok(())).is_err());
        controller.step(&event("click", Value::Bool(false)), &s(i64::MAX.to_string()), |_| Ok(())).unwrap();
        assert_eq!(controller.state().get("identifier").and_then(Value::as_str), Some("9223372036854775807"));
    }
    #[test] fn bundled_controller_runs_real_navigation_theme_drafts_and_cancel() {
        let mut controller = Controller::new(&bundled_source(), bundled_widgets()).unwrap();
        let observed = crate::settings_script_bridge::observation(&crate::settings_app::SettingsSnapshot::default(), false).unwrap();
        let ev = |kind, id| obj(vec![("kind", s(kind)), ("id", s(id)), ("value", Value::Null)]);
        controller.step(&ev("observe", ""), &observed, |_| Ok(())).unwrap();
        controller.step(&ev("click", "appearance_page"), &observed, |_| Ok(())).unwrap();
        assert_eq!(controller.state().get("page").and_then(Value::as_str), Some("Appearance"));
        let (draft, ()) = controller.step(&ev("click", "paper"), &observed, |_| Ok(())).unwrap();
        assert!(draft.requests.is_empty());
        assert_eq!(draft.state.get("theme_draft").unwrap().get("preset").and_then(Value::as_str), Some("paper"));
        let (apply, requests) = controller.step(&ev("click", "apply_theme"), &observed, |frame| {
            frame.requests.iter().map(|r| crate::settings_script_bridge::basic_request(r, &crate::settings_app::SettingsSnapshot::default()).ok_or_else(|| "Unknown native request".into())).collect::<Result<Vec<_>, String>>()
        }).unwrap();
        assert_eq!(apply.requests.len(), 1);
        assert!(matches!(requests[0], crate::settings_app::SettingsRequest::Theme(crate::mobile_theme::Selection { preset: crate::mobile_theme::Preset::Paper, .. })));
        controller.step(&ev("click", "cancel_theme"), &observed, |_| Ok(())).unwrap();
        assert_eq!(controller.state().get("theme_draft").unwrap().get("preset").and_then(Value::as_str), Some("octosense"));
        controller.step(&ev("back", ""), &observed, |_| Ok(())).unwrap();
        assert_eq!(controller.state().get("page").and_then(Value::as_str), Some("Overview"));
    }
}
