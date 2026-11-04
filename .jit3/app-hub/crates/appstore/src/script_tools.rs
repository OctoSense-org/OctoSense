//! Admitted script tools execute on their existing full-app Splash isolate.
//!
//! ABI: `fn app_tool(name, call_id)` reads `mod.app_tools.request(call_id)`
//! and settles with `mod.app_tools.complete(call_id, result)` or `.fail`.
//! Calls never evaluate source from a model, start a second VM, or select an
//! app/account from their arguments. A closed app fails with app_not_running.
use makepad_widgets::widget_async::CxSplashVmExt;
use makepad_widgets::*;
use octosense_app_policy::{AgentBundle, ImplementedBy, ToolSpec};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const MAX_PENDING: usize = 128;
const MAX_PER_APP: usize = 16;
const MAX_BYTES: usize = 1 << 20;
const MAX_TIMEOUT: Duration = Duration::from_secs(60);
pub type Completion = Box<dyn FnOnce(Result<Value, String>) + Send>;

#[derive(Clone)]
struct Surface {
    heap: usize,
    generation: u64,
    account: String,
    tools: HashMap<String, ToolSpec>,
}
struct Pending {
    app: String,
    heap: usize,
    generation: u64,
    name: String,
    account: String,
    caller: String,
    args: Value,
    deadline: Instant,
    started: bool,
    completion: Completion,
}
#[derive(Default)]
struct State {
    surfaces: HashMap<String, Surface>,
    pending: HashMap<String, Pending>,
    next: u64,
}
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
fn state() -> std::sync::MutexGuard<'static, State> {
    STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Held only by the admitted full-app runner. Dropping it revokes queued and
/// async calls even if another copy of the app's source exists in Glance.
pub struct Registration {
    app: String,
    generation: u64,
}
impl Drop for Registration {
    fn drop(&mut self) {
        let callbacks = {
            let mut s = state();
            if s.surfaces
                .get(&self.app)
                .is_some_and(|v| v.generation == self.generation)
            {
                s.surfaces.remove(&self.app);
            }
            let keys: Vec<_> = s
                .pending
                .iter()
                .filter(|(_, p)| p.app == self.app && p.generation == self.generation)
                .map(|(id, _)| id.clone())
                .collect();
            keys.into_iter()
                .filter_map(|id| s.pending.remove(&id))
                .map(|p| p.completion)
                .collect::<Vec<_>>()
        };
        for callback in callbacks {
            callback(Err("app_not_running: the owning app closed".into()));
        }
    }
}

/// Only native mounting code calls this, after policy application and source
/// loading from the same admitted bundle. A second active owner is refused.
pub fn bind(
    cx: &mut Cx,
    app: &str,
    bundle: &Path,
    card: &SplashRef,
) -> Result<Option<Registration>, String> {
    let manifest = octosense_app_policy::AppManifest::parse(
        &std::fs::read_to_string(bundle.join("manifest.json")).map_err(|e| e.to_string())?,
    )?;
    if manifest.id != app {
        return Err("tool owner differs from admitted manifest".into());
    }
    let Some(agent) = AgentBundle::load(bundle, &manifest)? else {
        return Ok(None);
    };
    let tools: HashMap<_, _> = agent
        .tools
        .into_iter()
        .filter(|t| t.implemented_by == ImplementedBy::App)
        .map(|t| (t.name.clone(), t))
        .collect();
    if tools.is_empty() {
        return Ok(None);
    }
    if !manifest
        .requires
        .iter()
        .any(|feature| feature == "script-tools-v1")
    {
        return Err("Script tool apps must declare requires: [\"script-tools-v1\"]".into());
    }
    let heap = card
        .isolate_heap_key(cx)
        .ok_or("app_not_running: script did not start")?;
    bind_tools(app, heap, tools).map(Some)
}
fn bind_tools(
    app: &str,
    heap: usize,
    tools: HashMap<String, ToolSpec>,
) -> Result<Registration, String> {
    let mut s = state();
    if s.surfaces.contains_key(app) {
        return Err("app_busy: another full-app instance owns the tools".into());
    }
    s.next += 1;
    let generation = s.next;
    s.surfaces.insert(
        app.into(),
        Surface {
            heap,
            generation,
            account: "device".into(),
            tools,
        },
    );
    Ok(Registration {
        app: app.into(),
        generation,
    })
}

/// Called by the authorized shell relay. Identity comes from its admission and
/// account broker, never from `args`. Returns the token used for cancellation.
pub fn submit(
    app: &str,
    name: &str,
    args: Value,
    account: &str,
    caller: &str,
    timeout: Duration,
    completion: Completion,
) -> Result<String, String> {
    if args.to_string().len() > MAX_BYTES {
        return Err("invalid_arguments: tool arguments exceed 1 MiB".into());
    }
    let mut s = state();
    let surface = s
        .surfaces
        .get(app)
        .cloned()
        .ok_or("app_not_running: open this app before calling its script tools")?;
    if surface.account != account {
        return Err("account_scope: app account changed".into());
    }
    let tool = surface
        .tools
        .get(name)
        .ok_or("tool_not_declared: the running bundle does not implement this tool")?;
    crate::tool_schema::check(&tool.input_schema, &args)
        .map_err(|e| format!("invalid_arguments: {e}"))?;
    if s.pending.len() >= MAX_PENDING
        || s.pending.values().filter(|p| p.app == app).count() >= MAX_PER_APP
    {
        return Err("app_busy: too many script tools are pending".into());
    }
    s.next += 1;
    let token = format!("script-tool-{}", s.next);
    s.pending.insert(
        token.clone(),
        Pending {
            app: app.into(),
            heap: surface.heap,
            generation: surface.generation,
            name: name.into(),
            account: account.into(),
            caller: caller.into(),
            args,
            deadline: Instant::now() + timeout.min(MAX_TIMEOUT),
            started: false,
            completion,
        },
    );
    drop(s);
    SignalToUI::set_ui_signal();
    start_sweeper();
    Ok(token)
}

/// Update from the host account broker, never from an app request. Calls on
/// the old account are revoked before the new account receives tools.
pub fn set_account(app: &str, account: &str) {
    let cancelled = {
        let mut s = state();
        let Some(surface) = s.surfaces.get_mut(app) else {
            return;
        };
        if surface.account == account {
            return;
        }
        surface.account = account.into();
        let keys: Vec<_> = s
            .pending
            .iter()
            .filter(|(_, p)| p.app == app)
            .map(|(id, _)| id.clone())
            .collect();
        keys.into_iter()
            .filter_map(|id| s.pending.remove(&id))
            .collect::<Vec<_>>()
    };
    for p in cancelled {
        (p.completion)(Err("account_scope: the app account changed".into()));
    }
}

pub fn cancel(token: &str) {
    let pending = state().pending.remove(token);
    if let Some(p) = pending {
        (p.completion)(Err("cancelled: script tool was cancelled".into()));
    }
}

fn expire(now: Instant) {
    let expired = {
        let mut s = state();
        let ids: Vec<_> = s
            .pending
            .iter()
            .filter(|(_, p)| p.deadline <= now)
            .map(|(id, _)| id.clone())
            .collect();
        ids.into_iter()
            .filter_map(|id| s.pending.remove(&id))
            .collect::<Vec<_>>()
    };
    for p in expired {
        (p.completion)(Err(
            "timeout: script tool did not finish before its deadline".into(),
        ));
    }
}
fn start_sweeper() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static START: std::sync::Once = std::sync::Once::new();
        START.call_once(|| {
            std::thread::spawn(|| loop {
                std::thread::sleep(Duration::from_millis(100));
                expire(Instant::now());
            });
        });
    }
}

/// The runner checks this before delivering any app event or callback. It
/// combines this suppress-only bit with its own admitted foreground policy.
pub fn pending_for(cx: &mut Cx, app: &str, card: &SplashRef) -> bool {
    let Some(heap) = card.isolate_heap_key(cx) else { return false };
    let s = state();
    let Some(owner) = s.surfaces.get(app).filter(|owner| owner.heap == heap) else { return false };
    s.pending.values().any(|p| p.app == app && p.heap == heap && p.generation == owner.generation)
}

/// Run from the owning runner on UI events, outside draw. The framework's
/// entry limit and cumulative isolate instruction budget bound each hook.
pub fn pump(cx: &mut Cx, app: &str, card: &SplashRef) {
    if cx.in_draw_event() {
        return;
    }
    expire(Instant::now());
    let Some(heap) = card.isolate_heap_key(cx) else {
        return;
    };
    let jobs = {
        let mut s = state();
        let Some(owner) = s.surfaces.get(app).filter(|v| v.heap == heap).cloned() else {
            return;
        };
        s.pending
            .iter_mut()
            .filter(|(_, p)| {
                p.app == app && p.heap == heap && p.generation == owner.generation && !p.started
            })
            .map(|(token, p)| {
                p.started = true;
                (token.clone(), p.name.clone())
            })
            .collect::<Vec<_>>()
    };
    // Never grant prompt authority here. Only the runner knows whether its
    // admitted policy and foreground lifecycle allow prompts. Keep suppression
    // through completion callbacks for this event, even for a synchronous hook.
    if pending_for(cx, app, card) { card.set_host_prompts(cx, false); }
    for (token, name) in jobs {
        if !is_active(heap, &token) {
            continue;
        }
        let vm_id = card
            .borrow()
            .and_then(|splash| cx.script_ref_vm_id(&splash.view.source));
        if let Some(vm_id) = vm_id {
            cx.with_script_vm_id(vm_id, |vm| {
                vm.take_errors();
                // Catch what the handler's run raises. Without a sink the VM
                // drains it to the log, so a runaway that the per-evaluation
                // instruction limit stopped would leave no error here.
                vm.bx.captured_errors = Some(Vec::new());
            });
        }
        let called = card.call_script_fn_with_strings(cx, id!(app_tool), &[&name, &token]);
        let errors = vm_id
            .map(|vm_id| cx.with_script_vm_id(vm_id, |vm| vm.take_errors()))
            .unwrap_or_default();
        for error in &errors {
            log!("app tool {name} in {app}: {error}");
        }
        if !called {
            finish(
                heap,
                &token,
                Err(
                    "app_handler_missing: define fn app_tool(name, call_id) in the app source"
                        .into(),
                ),
            );
        } else if vm_id.is_some() {
            let failed = !errors.is_empty() || !makepad_widgets::splash_policy::may_run(heap);
            if failed {
                finish(
                    heap,
                    &token,
                    Err(
                        "app_error: script handler failed or exceeded its instruction budget"
                            .into(),
                    ),
                );
            }
        }
    }
}
fn is_active(heap: usize, token: &str) -> bool {
    state()
        .pending
        .get(token)
        .is_some_and(|p| p.heap == heap && p.deadline > Instant::now())
}
fn request(heap: usize, token: &str) -> Option<Value> {
    state().pending.get(token).filter(|p|p.heap == heap && p.started && p.deadline > Instant::now()).map(|p|json!({"args":p.args,"context":{"app":p.app,"account":p.account,"caller":p.caller,"call_id":token}}))
}
fn finish(heap: usize, token: &str, result: Result<Value, String>) -> bool {
    let pending = {
        let mut s = state();
        let Some(p) = s
            .pending
            .get(token)
            .filter(|p| p.heap == heap && p.started && p.deadline > Instant::now())
        else {
            return false;
        };
        let Some(tool) = s
            .surfaces
            .get(&p.app)
            .filter(|surface| surface.generation == p.generation)
            .and_then(|surface| surface.tools.get(&p.name))
        else {
            return false;
        };
        let result = result.and_then(|value| {
            if value.to_string().len() > MAX_BYTES {
                return Err("invalid_result: result exceeds 1 MiB".into());
            }
            crate::tool_schema::check(&tool.output_schema, &value)
                .map_err(|e| format!("invalid_result: {e}"))?;
            Ok(value)
        });
        s.pending.remove(token).map(|p| (p.completion, result))
    };
    if let Some((callback, result)) = pending {
        callback(result);
        SignalToUI::set_ui_signal();
        true
    } else {
        false
    }
}

fn read_token(vm: &mut ScriptVm, value: ScriptValue) -> Option<String> {
    vm.string_with(value, |_, token| (token.len() <= 128).then(|| token.to_owned())).flatten()
}

/// No service capability is conferred by this module. A reply token only
/// identifies a live call in this exact heap; another app or host sheet cannot
/// read its arguments or complete it, even if it guesses the token.
pub fn script_mod(vm: &mut ScriptVm) {
    let module = vm.new_module(id!(app_tools));
    vm.add_method(
        module,
        id_lut!(request),
        script_args_def!(call_id = NIL),
        |vm, args| {
            let value = script_value!(vm, args.call_id);
            let Some(token) = read_token(vm, value) else { return NIL };
            let Some(data) = request(vm.bx.heap.heap_key(), &token) else {
                return NIL;
            };
            makepad_widgets::makepad_script::json::JsonParserThread::default()
                .read_json(&data.to_string(), &mut vm.bx.heap)
        },
    );
    vm.add_method(
        module,
        id_lut!(active),
        script_args_def!(call_id = NIL),
        |vm, args| {
            let value = script_value!(vm, args.call_id);
            let Some(token) = read_token(vm, value) else { return false.into() };
            is_active(vm.bx.heap.heap_key(), &token).into()
        },
    );
    vm.add_method(
        module,
        id_lut!(complete),
        script_args_def!(call_id = NIL, result = NIL),
        |vm, args| {
            let value = script_value!(vm, args.call_id);
            let result = script_value!(vm, args.result);
            let Some(token) = read_token(vm, value) else { return false.into() };
            let result = crate::script_tools_json::result(&vm.bx.heap, result, MAX_BYTES);
            finish(vm.bx.heap.heap_key(), &token, result).into()
        },
    );
    vm.add_method(
        module,
        id_lut!(fail),
        script_args_def!(call_id = NIL, error = NIL),
        |vm, args| {
            let value = script_value!(vm, args.call_id);
            let error = script_value!(vm, args.error);
            let Some(token) = read_token(vm, value) else { return false.into() };
            let mut message = String::new();
            vm.string_with(error, |_, s| message.extend(s.chars().take(2048)));
            finish(
                vm.bx.heap.heap_key(),
                &token,
                Err(format!("app_error: {message}")),
            )
            .into()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    fn spec() -> ToolSpec {
        serde_json::from_value(json!({
        "name":"fixture.add", "description":"Add to this app's state", "implemented_by":"app", "risk":"act",
        "input_schema":{"type":"object","properties":{"amount":{"type":"integer"}},"required":["amount"],"additionalProperties":false},
        "output_schema":{"type":"object","properties":{"total":{"type":"integer"},"account":{"type":"string"}},"required":["total","account"],"additionalProperties":false}
    })).unwrap()
    }
    fn fixture(app: &str, body: &str) -> (Cx, SplashRef, Registration) {
        crate::register_card_vocabulary();
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let root = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            let value = script_eval!(vm, { use mod.widgets.* View { card := Splash{} } });
            WidgetRef::script_from_value(vm, value)
        });
        let card = root.splash(&mut cx, ids!(card));
        card.set_host_tag(&mut cx, Some(app.into()));
        card.set_policy(&mut cx, Some(vec![]), Some(100_000));
        card.set_text(&mut cx, body);
        let heap = card.isolate_heap_key(&mut cx).unwrap();
        let registration = bind_tools(app, heap, [("fixture.add".into(), spec())].into()).unwrap();
        (cx, card, registration)
    }
    type Received = Arc<Mutex<Vec<Result<Value, String>>>>;
    fn call(app: &str, args: Value) -> (String, Received) {
        let replies: Received = Default::default();
        let out = replies.clone();
        let token = submit(
            app,
            "fixture.add",
            args,
            "device",
            "system",
            Duration::from_secs(60),
            Box::new(move |r| out.lock().unwrap().push(r)),
        )
        .unwrap();
        (token, replies)
    }
    const BODY: &str = r#"
        let total = 10
        fn app_tool(name, call_id) {
            let request = mod.app_tools.request(call_id)
            total = total + request.args.amount
            mod.app_tools.complete(call_id, {total: total, account: request.context.account})
        }
        Label {text: "fixture"}
    "#;
    #[test]
    fn the_runner_binds_only_matching_digest_checked_declarations() {
        let app = "org.example.fixture";
        let (mut cx, card, temporary) = fixture(app, BODY);
        drop(temporary);
        let dir =
            std::env::temp_dir().join(format!("script-tools-admission-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.splash"), BODY).unwrap();
        std::fs::write(
            dir.join("tools.json"),
            json!({"schema":1,"tools":[spec()]}).to_string(),
        )
        .unwrap();
        let mut manifest = json!({"schema":1,"id":app,"name":"Fixture","version":"0.1.0","capabilities":["storage"],"requires":["script-tools-v1"],"integrity":{"bundle_blake3":""}});
        std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        manifest["integrity"]["bundle_blake3"] =
            json!(octosense_app_policy::digest_dir(&dir).unwrap());
        std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        let registration = bind(&mut cx, app, &dir, &card).unwrap().unwrap();
        let (_, replies) = call(app, json!({"amount":2}));
        pump(&mut cx, app, &card);
        assert_eq!(replies.lock().unwrap()[0].as_ref().unwrap()["total"], 12);
        drop(registration);
        assert!(bind(&mut cx, "org.example.wrong", &dir, &card).is_err());
        std::fs::write(dir.join("main.splash"), "Label{text: \"tampered\"}").unwrap();
        assert!(bind(&mut cx, app, &dir, &card).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn invokes_admitted_handler_in_live_ui_state_and_checks_both_schemas() {
        let app = "org.example.tool-state";
        let (mut cx, card, _registration) = fixture(app, BODY);
        let (_, replies) = call(app, json!({"amount":3}));
        pump(&mut cx, app, &card);
        assert_eq!(
            replies.lock().unwrap().as_slice(),
            &[Ok(json!({"total":13,"account":"device"}))]
        );
        let (_, replies) = call(app, json!({"amount":2}));
        pump(&mut cx, app, &card);
        assert_eq!(replies.lock().unwrap()[0].as_ref().unwrap()["total"], 15);
        assert!(submit(
            app,
            "fixture.add",
            json!({"amount":"bad"}),
            "device",
            "system",
            Duration::from_secs(1),
            Box::new(|_| {})
        )
        .unwrap_err()
        .contains("invalid_arguments"));
        card.set_text(&mut cx, r#"fn app_tool(name, call_id) { mod.app_tools.complete(call_id, {total: "wrong", account: "device"}) } Label{text:"bad"}"#);
        let (_, replies) = call(app, json!({"amount":1}));
        pump(&mut cx, app, &card);
        assert!(replies.lock().unwrap()[0]
            .as_ref()
            .unwrap_err()
            .contains("invalid_result"));
    }
    #[test]
    fn closed_missing_or_undeclared_tools_fail_and_duplicate_owners_are_refused() {
        let app = "org.example.tool-lifecycle";
        assert!(submit(
            app,
            "fixture.add",
            json!({"amount":1}),
            "device",
            "system",
            Duration::from_secs(1),
            Box::new(|_| {})
        )
        .unwrap_err()
        .contains("app_not_running"));
        let (mut cx, card, registration) = fixture(app, "Label{text: \"missing\"}");
        assert!(bind_tools(app, card.isolate_heap_key(&mut cx).unwrap(), HashMap::new()).is_err());
        assert!(submit(
            app,
            "fixture.undeclared",
            json!({}),
            "device",
            "system",
            Duration::from_secs(1),
            Box::new(|_| {})
        )
        .unwrap_err()
        .contains("tool_not_declared"));
        let (_, replies) = call(app, json!({"amount":1}));
        pump(&mut cx, app, &card);
        assert!(replies.lock().unwrap()[0]
            .as_ref()
            .unwrap_err()
            .contains("app_handler_missing"));
        let (_, replies) = call(app, json!({"amount":1}));
        drop(registration);
        assert!(replies.lock().unwrap()[0]
            .as_ref()
            .unwrap_err()
            .contains("app_not_running"));
    }
    #[test]
    fn cancellation_deadline_and_account_change_prevent_queued_execution() {
        let app = "org.example.tool-cancel";
        let (mut cx, card, _registration) = fixture(app, BODY);
        let (token, replies) = call(app, json!({"amount":100}));
        cancel(&token);
        pump(&mut cx, app, &card);
        assert!(replies.lock().unwrap()[0]
            .as_ref()
            .unwrap_err()
            .contains("cancelled"));
        let (token, replies) = call(app, json!({"amount":100}));
        {
            state().pending.get_mut(&token).unwrap().deadline = Instant::now();
        }
        pump(&mut cx, app, &card);
        assert!(replies.lock().unwrap()[0]
            .as_ref()
            .unwrap_err()
            .contains("timeout"));
        let (_, replies) = call(app, json!({"amount":100}));
        set_account(app, "other-account");
        pump(&mut cx, app, &card);
        assert!(replies.lock().unwrap()[0]
            .as_ref()
            .unwrap_err()
            .contains("account_scope"));
        assert!(submit(
            app,
            "fixture.add",
            json!({"amount":1}),
            "device",
            "system",
            Duration::from_secs(1),
            Box::new(|_| {})
        )
        .unwrap_err()
        .contains("account_scope"));
        set_account(app, "device");
        let (_, replies) = call(app, json!({"amount":1}));
        pump(&mut cx, app, &card);
        assert_eq!(replies.lock().unwrap()[0].as_ref().unwrap()["total"], 11);
    }
    #[test]
    fn ui_handler_and_agent_tool_share_the_same_storage_jail() {
        let app = "org.example.tool-storage";
        let body = r#"
            fn edit_note(value) { fs.write("note.txt", value) }
            fn app_tool(name, call_id) {
                let text = fs.read("note.txt")
                mod.app_tools.complete(call_id, {total: 1, account: text})
            }
            Label{text: "storage fixture"}
        "#;
        let (mut cx, card, _registration) = fixture(app, body);
        let jail =
            std::env::temp_dir().join(format!("script-tools-storage-{}", std::process::id()));
        std::fs::create_dir_all(&jail).unwrap();
        card.set_sandbox_dir(&mut cx, Some(jail.clone()));
        assert!(card.call_script_fn_with_strings(&mut cx, id!(edit_note), &["written by UI"]));
        let (_, replies) = call(app, json!({"amount":1}));
        pump(&mut cx, app, &card);
        assert_eq!(
            replies.lock().unwrap()[0].as_ref().unwrap()["account"],
            "written by UI"
        );
        assert_eq!(
            std::fs::read_to_string(jail.join("note.txt")).unwrap(),
            "written by UI"
        );
        let _ = std::fs::remove_dir_all(jail);
    }
    #[test]
    fn tool_requests_cannot_open_foreground_permission_sheets() {
        let app = "org.example.tool-prompt";
        let body = r#"
            fn app_tool(name, call_id) {
                host.request("fixture.permission", {}, fn(result){})
                mod.app_tools.complete(call_id,{total:1,account:"device"})
            }
            Label{text:"prompt fixture"}
        "#;
        let (mut cx, card, _registration) = fixture(app, body);
        card.set_host_caps(&mut cx, vec!["fixture".into()]);
        card.set_policy(&mut cx, Some(vec![]), Some(100_000));
        let heap = card.isolate_heap_key(&mut cx).unwrap();
        let (_, replies) = call(app, json!({"amount":1}));
        pump(&mut cx, app, &card);
        assert!(replies.lock().unwrap()[0].is_ok());
        let requests = makepad_widgets::splash_host::take_splash_host_requests_for(&[heap]);
        assert_eq!(requests.len(), 1);
        assert!(
            !requests[0].may_prompt,
            "a visible app does not make agent input physical input"
        );
    }
    #[test]
    fn tool_result_graphs_are_bounded_before_host_allocation() {
        for (suffix, code, expected) in [
            ("cycle", "let data = {}; data.self = data", "cyclic"),
            ("shared", "let data = [1]; for i in 0..25 { data = [data, data] }", "exceeds 1 MiB"),
        ] {
            let app = format!("org.example.tool-json-{suffix}");
            let body = format!("fn app_tool(name, call_id) {{ {code}; mod.app_tools.complete(call_id, data) }} Label{{text: \"bounded JSON\"}}");
            let (mut cx, card, _registration) = fixture(&app, &body);
            let (_, replies) = call(&app, json!({"amount":1}));
            pump(&mut cx, &app, &card);
            let replies = replies.lock().unwrap();
            let error = replies[0].as_ref().unwrap_err();
            assert!(error.contains(expected), "{suffix}: {error}");
        }
    }

    #[test]
    fn pump_never_grants_prompt_authority_to_a_denied_or_unowned_surface() {
        let app = "org.example.tool-denied-prompt";
        let body = r#"fn probe() { host.request("fixture.permission", {}, fn(r){}) } Label{text:"denied"}"#;
        let (mut cx, card, registration) = fixture(app, body);
        card.set_host_caps(&mut cx, vec!["fixture".into()]);
        card.set_policy(&mut cx, Some(vec![]), Some(100_000));
        let heap = card.isolate_heap_key(&mut cx).unwrap();
        for owner in [true, false] {
            card.set_host_prompts(&mut cx, false);
            pump(&mut cx, if owner { app } else { "org.example.other-owner" }, &card);
            card.call_script_fn_with_strings(&mut cx, id!(probe), &[]);
            let requests = makepad_widgets::splash_host::take_splash_host_requests_for(&[heap]);
            assert_eq!(requests.len(), 1);
            assert!(!requests[0].may_prompt, "pump enlarged prompt authority");
        }
        drop(registration);
        pump(&mut cx, app, &card);
        card.call_script_fn_with_strings(&mut cx, id!(probe), &[]);
        assert!(!makepad_widgets::splash_host::take_splash_host_requests_for(&[heap])[0].may_prompt);
    }

    /// The per-evaluation instruction limit stops the loop, and the call
    /// fails at once. (Since makepad#117 the app's cumulative budget is a
    /// declaration and stops nothing.)
    #[test]
    fn an_unbounded_script_loop_is_stopped_by_the_vm_entry_budget() {
        let app = "org.example.tool-budget";
        let (mut cx, card, _registration) = fixture(
            app,
            "fn app_tool(name, call_id) { loop {} } Label{text: \"bounded\"}",
        );
        let (token, replies) = call(app, json!({"amount":1}));
        pump(&mut cx, app, &card);
        assert!(card.instructions_used(&mut cx) > 100_000);
        assert!(!is_active(card.isolate_heap_key(&mut cx).unwrap(), &token));
        assert!(replies.lock().unwrap()[0]
            .as_ref()
            .unwrap_err()
            .contains("instruction budget"));
    }
    #[test]
    fn another_heap_cannot_read_or_complete_async_calls_and_late_results_are_dropped() {
        let app = "org.example.tool-isolation";
        let (mut cx, card, registration) = fixture(
            app,
            "fn app_tool(name, call_id) {} Label{text: \"waiting\"}",
        );
        let heap = card.isolate_heap_key(&mut cx).unwrap();
        let (token, replies) = call(app, json!({"amount":1}));
        pump(&mut cx, app, &card);
        assert!(request(heap, &token).is_some());
        assert!(request(heap.wrapping_add(1), &token).is_none());
        assert!(!finish(
            heap.wrapping_add(1),
            &token,
            Ok(json!({"total":1,"account":"device"}))
        ));
        assert!(replies.lock().unwrap().is_empty());
        assert!(finish(
            heap,
            &token,
            Ok(json!({"total":1,"account":"device"}))
        ));
        assert!(!finish(
            heap,
            &token,
            Ok(json!({"total":2,"account":"device"}))
        ));
        assert_eq!(replies.lock().unwrap().len(), 1);
        let (late, replies) = call(app, json!({"amount":1}));
        pump(&mut cx, app, &card);
        drop(registration);
        assert!(!finish(
            heap,
            &late,
            Ok(json!({"total":3,"account":"device"}))
        ));
        assert_eq!(replies.lock().unwrap().len(), 1);
    }
}
