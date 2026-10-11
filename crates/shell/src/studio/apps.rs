//! Developer app runner. Real visible Splash widgets, private app storage, and
//! Makepad's in-process instrument; no global widget-tree or remote server swap.
use super::{rgba, MAX_PNG};
use crate::dev_mode::{self, DevTag};
use makepad_widgets::makepad_platform::event::{
    ScrollEvent, TouchPoint, TouchState, TouchUpdateEvent,
};
use makepad_widgets::{widget_tree::WidgetTree, *};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_REQUESTS: usize = 16;
const MAX_INSTANCES: usize = 4;
static FOREGROUND: AtomicBool = AtomicBool::new(true);

/// Constructed only by the host's admitted, immutable bundle staging path.
#[derive(Clone)]
pub struct OpenSpec {
    pub instance_id: String,
    pub app_id: Option<String>,
    pub owner: String,
    pub dev_tag: DevTag,
    pub source: String,
    pub jail: PathBuf,
    pub persistent: bool,
    pub title: String,
    pub storage_quota: u64,
    pub instruction_budget: u64,
    pub memory_bytes: u64,
}
pub enum Action {
    Ready,
    Input {
        widget_id: String,
        kind: String,
        text: Option<String>,
        delta_y: Option<f64>,
    },
    Inspect {
        output: File,
    },
    Close,
}
pub struct Request {
    pub id: String,
    pub owner: String,
    pub dev_tag: DevTag,
    pub instance_id: String,
    pub action: Action,
    pub reply: Sender<Result<Value, String>>,
}
struct Pending {
    request: Request,
    started: Instant,
    cancelled: Arc<AtomicBool>,
}
#[derive(Default)]
struct Queue {
    specs: HashMap<String, OpenSpec>,
    launches: VecDeque<String>,
    closes: VecDeque<String>,
    requests: VecDeque<Pending>,
    tokens: HashMap<String, Arc<AtomicBool>>,
    request_instances: HashMap<String, String>,
    failures: HashMap<String, (String, DevTag, String)>,
}
fn queue() -> &'static Mutex<Queue> {
    static QUEUE: OnceLock<Mutex<Queue>> = OnceLock::new();
    QUEUE.get_or_init(Default::default)
}
const GRANT_CACHE_TTL: Duration = Duration::from_secs(2);
/// Whether `owner` still holds a developer grant under `tag`. Developer mode
/// is asked once per grant per generation (and at most every two seconds),
/// so the UI thread rarely takes its lock.
fn granted(owner: &str, tag: &DevTag) -> bool {
    type Cache = (u64, Instant, HashMap<(String, String, u64), bool>);
    static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
    let generation = dev_mode::generation_relaxed();
    let key = (owner.to_owned(), tag.profile_id.clone(), tag.since);
    {
        let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((cached, filled, answers)) = cache.as_ref() {
            if *cached == generation && filled.elapsed() < GRANT_CACHE_TTL {
                if let Some(answer) = answers.get(&key) {
                    return *answer;
                }
            }
        }
    }
    let identity: Option<Value> = serde_json::from_str(owner).ok();
    let app = identity
        .as_ref()
        .and_then(|v| v["app"].as_str())
        .unwrap_or(owner);
    let answer =
        dev_mode::tag_valid(tag) && dev_mode::grants_all(crate::host_tools::app_of_peer(app));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    match cache.as_mut() {
        Some((cached, filled, answers))
            if *cached == generation && filled.elapsed() < GRANT_CACHE_TTL =>
        {
            answers.insert(key, answer);
        }
        _ => *cache = Some((generation, Instant::now(), HashMap::from([(key, answer)]))),
    }
    answer
}
/// Whether an instance of the installed app `app_id` is open or launching.
pub fn app_is_open(app_id: &str) -> bool {
    queue()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .specs
        .values()
        .any(|s| s.app_id.as_deref() == Some(app_id))
}
fn valid(p: &Pending) -> Result<(), String> {
    if !FOREGROUND.load(Ordering::Acquire) {
        return Err("not_foreground".into());
    }
    if p.cancelled.load(Ordering::Acquire) {
        return Err("studio_cancelled".into());
    }
    if !granted(&p.request.owner, &p.request.dev_tag) {
        return Err("studio_grant_expired".into());
    }
    if p.started.elapsed() >= REQUEST_TIMEOUT {
        return Err("studio_timeout".into());
    }
    Ok(())
}
fn finish(p: Pending, result: Result<Value, String>) {
    let id = p.request.id.clone();
    let _ = p.request.reply.send(result);
    let mut q = queue().lock().unwrap_or_else(|e| e.into_inner());
    q.tokens.remove(&id);
    q.request_instances.remove(&id);
}
/// A close acknowledgement is a cleanup barrier for a caller reopening the app.
fn finish_after_cleanup(p: Pending, cleanup: impl FnOnce()) {
    cleanup();
    finish(p, Ok(json!({"closed":true})));
}
pub fn stage_open(spec: OpenSpec) -> Result<(), String> {
    if !FOREGROUND.load(Ordering::Acquire) {
        return Err("not_foreground".into());
    }
    if !granted(&spec.owner, &spec.dev_tag) {
        return Err("studio_grant_expired".into());
    }
    if spec.source.len() > 256 * 1024 || spec.source.trim().is_empty() {
        return Err("studio_source_size".into());
    }
    if spec.storage_quota > 1024 * 1024
        || spec.instruction_budget == 0
        || spec.instruction_budget > 5_000_000
        || spec.memory_bytes == 0
        || spec.memory_bytes > 16 * 1024 * 1024
    {
        return Err("studio_resource_limits".into());
    }
    // Initial script-app support deliberately has no external resource route.
    // This is also enforced by the isolate; this check makes refusal explicit.
    if spec.source.contains("{{assets}}") || spec.source.contains("http_resource(") {
        return Err("studio_bundle_assets_unsupported".into());
    }
    let mut q = queue().lock().unwrap_or_else(|e| e.into_inner());
    if q.specs.len() >= MAX_INSTANCES || q.specs.contains_key(&spec.instance_id) {
        return Err("studio_busy".into());
    }
    if spec.app_id.as_ref().is_some_and(|id| {
        q.specs
            .values()
            .any(|open| open.app_id.as_ref() == Some(id))
    }) {
        return Err("studio_app_already_open".into());
    }
    q.launches.push_back(spec.instance_id.clone());
    q.specs.insert(spec.instance_id.clone(), spec);
    SignalToUI::set_ui_signal();
    Ok(())
}
pub fn take_launches() -> Vec<String> {
    queue()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .launches
        .drain(..)
        .collect()
}
pub fn take_closes() -> Vec<String> {
    queue()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .closes
        .drain(..)
        .collect()
}
pub fn launch_info(instance_id: &str) -> Option<(Option<String>, String)> {
    queue()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .specs
        .get(instance_id)
        .map(|s| (s.app_id.clone(), s.title.clone()))
}
pub fn submit(request: Request) -> Result<(), String> {
    if !FOREGROUND.load(Ordering::Acquire) {
        return Err("not_foreground".into());
    }
    if !granted(&request.owner, &request.dev_tag) {
        return Err("studio_grant_expired".into());
    }
    let mut q = queue().lock().unwrap_or_else(|e| e.into_inner());
    if !q.specs.contains_key(&request.instance_id) {
        if let Some((owner, tag, message)) = q.failures.get(&request.instance_id) {
            if owner == &request.owner && tag == &request.dev_tag {
                return Err(message.clone());
            }
        }
        return Err("studio_instance_not_found".into());
    }
    let spec = &q.specs[&request.instance_id];
    if spec.owner != request.owner || spec.dev_tag != request.dev_tag {
        return Err("studio_instance_scope".into());
    }
    if q.tokens.len() >= MAX_REQUESTS || q.tokens.contains_key(&request.id) {
        return Err("studio_busy".into());
    }
    let token = Arc::new(AtomicBool::new(false));
    q.tokens.insert(request.id.clone(), token.clone());
    q.request_instances
        .insert(request.id.clone(), request.instance_id.clone());
    q.requests.push_back(Pending {
        request,
        started: Instant::now(),
        cancelled: token,
    });
    SignalToUI::set_ui_signal();
    Ok(())
}
/// Roll back an open that never reached its caller. Cleanup does not require a
/// still-valid grant, but the original owner and generation must match.
pub fn abandon(instance_id: &str, owner: &str, tag: &DevTag) -> Result<(), String> {
    let mut q = queue().lock().unwrap_or_else(|e| e.into_inner());
    let Some(spec) = q.specs.get(instance_id) else {
        return Ok(());
    };
    if spec.owner != owner || &spec.dev_tag != tag {
        return Err("studio_instance_scope".into());
    }
    if q.launches.iter().any(|id| id == instance_id) {
        let spec = q.specs.remove(instance_id).unwrap();
        q.launches.retain(|id| id != instance_id);
        drop(q);
        if !spec.persistent {
            let _ = std::fs::remove_dir_all(&spec.jail);
        }
    } else {
        if !q.closes.iter().any(|id| id == instance_id) {
            q.closes.push_back(instance_id.into());
        }
        for (id, instance) in &q.request_instances {
            if instance == instance_id {
                if let Some(token) = q.tokens.get(id) {
                    token.store(true, Ordering::Release);
                }
            }
        }
    }
    SignalToUI::set_ui_signal();
    Ok(())
}
pub fn cancel(id: &str) {
    if let Some(token) = queue()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .tokens
        .get(id)
    {
        token.store(true, Ordering::Release);
    }
    SignalToUI::set_ui_signal();
}
thread_local! {
    static ROOTS: RefCell<HashMap<String,WidgetRef>> = RefCell::new(HashMap::new());
    static CAPTURES: RefCell<Vec<Capture>> = const { RefCell::new(Vec::new()) };
}
struct Capture {
    pending: Pending,
    snapshot: Value,
    frozen: makepad_widgets::view::ViewTextureSnapshot,
    ticket: ReadbackTicket,
}

script_mod! {
    use mod.prelude.widgets.*
    mod.widgets.StudioApp = set_type_default() do #(StudioApp::register_widget(vm)) {
        width: Fill height: Fill flow: Overlay
        capture := CachedView {
            width: Fill height: Fill flow: Overlay
            card := Splash { width: Fill height: Fill }
        }
    }
}
#[derive(Script, ScriptHook, Widget)]
pub struct StudioApp {
    #[deref]
    view: View,
    #[rust]
    spec: Option<OpenSpec>,
    #[rust]
    started: bool,
    #[rust]
    draws: usize,
    #[rust]
    timer: Timer,
    #[rust]
    pending: Option<Pending>,
    #[rust]
    text_next: Option<String>,
    #[rust]
    input_draw_pending: bool,
}
pub fn create(vm: &mut ScriptVm, instance_id: &str) -> WidgetRef {
    let spec = queue()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .specs
        .get(instance_id)
        .cloned();
    let value = script_eval!(vm,{use mod.widgets.* StudioApp{}});
    let root = WidgetRef::script_from_value(vm, value);
    if let Some(mut app) = root.borrow_mut::<StudioApp>() {
        app.spec = spec;
    }
    ROOTS.with(|roots| roots.borrow_mut().insert(instance_id.into(), root.clone()));
    root
}
pub fn shutdown(cx: &mut Cx, instance_id: &str) {
    let root = ROOTS.with(|roots| roots.borrow_mut().remove(instance_id));
    if let Some(root) = root {
        if let Some(mut app) = root.borrow_mut::<StudioApp>() {
            app.stop(cx);
        }
    }
    let mut q = queue().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(spec) = q.specs.remove(instance_id) {
        // A preview that never got its widget (an expired launch) still has a
        // folder; a widget removes its own in `stop`.
        if !spec.persistent {
            let _ = std::fs::remove_dir_all(&spec.jail);
        }
    }
    q.launches.retain(|id| id != instance_id);
    let mut rejected = Vec::new();
    let mut keep = VecDeque::new();
    for p in q.requests.drain(..) {
        if p.request.instance_id == instance_id {
            rejected.push(p)
        } else {
            keep.push_back(p)
        }
    }
    q.requests = keep;
    drop(q);
    for p in rejected {
        finish(p, Err("studio_closed".into()));
    }
    CAPTURES.with(|captures| {
        for c in captures
            .borrow()
            .iter()
            .filter(|c| c.pending.request.instance_id == instance_id)
        {
            c.pending.cancelled.store(true, Ordering::Release);
            cx.cancel_texture_readback(c.ticket);
        }
    });
}
/// Called before shell event routes. Readback workers also keep this token.
pub fn tick(cx: &mut Cx, event: &Event) {
    match event {
        Event::Pause | Event::Background => FOREGROUND.store(false, Ordering::Release),
        Event::Resume | Event::Foreground => FOREGROUND.store(true, Ordering::Release),
        _ => {}
    }
    // Only these events change a queue or a grant's standing. Input and draw
    // events take neither the queue lock nor developer mode on the UI thread.
    if !matches!(
        event,
        Event::Signal
            | Event::Timer(_)
            | Event::Pause
            | Event::Background
            | Event::Resume
            | Event::Foreground
    ) {
        return;
    }
    let background = matches!(event, Event::Pause | Event::Background);
    if background {
        for token in queue()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .tokens
            .values()
        {
            token.store(true, Ordering::Release);
        }
    }
    // The grant checks run after the lock is released: developer mode has
    // its own lock, which an executor thread may hold while it waits for us.
    let grants: Vec<(String, String, DevTag)> = queue()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .specs
        .iter()
        .map(|(id, s)| (id.clone(), s.owner.clone(), s.dev_tag.clone()))
        .collect();
    for (id, owner, tag) in grants {
        if !granted(&owner, &tag) {
            shutdown(cx, &id);
        }
    }
    let mut expired_requests = Vec::new();
    let waiting: Vec<Pending> = queue()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .requests
        .drain(..)
        .collect();
    let mut keep = VecDeque::new();
    for p in waiting {
        if let Err(error) = valid(&p) {
            expired_requests.push((p, error));
        } else {
            keep.push_back(p);
        }
    }
    {
        // Requests submitted meanwhile queue behind the ones that waited.
        let mut q = queue().lock().unwrap_or_else(|e| e.into_inner());
        let newer: Vec<Pending> = q.requests.drain(..).collect();
        q.requests = keep;
        q.requests.extend(newer);
    }
    for (p, error) in expired_requests {
        finish(p, Err(error));
    }
    CAPTURES.with(|captures| {
        for c in captures.borrow().iter() {
            if valid(&c.pending).is_err() {
                cx.cancel_texture_readback(c.ticket);
            }
        }
    });
}
impl StudioApp {
    fn stop(&mut self, cx: &mut Cx) {
        let card = self.view.splash(cx, ids!(card));
        super::release_isolate(cx, &card);
        card.set_text(cx, "");
        cx.stop_timer(self.timer);
        if let Some(p) = self.pending.take() {
            finish(p, Err("studio_closed".into()));
        }
        if let Some(spec) = self.spec.take() {
            if !spec.persistent {
                let _ = std::fs::remove_dir_all(&spec.jail);
            }
            let mut q = queue().lock().unwrap_or_else(|e| e.into_inner());
            q.specs.remove(&spec.instance_id);
            for (id, instance) in &q.request_instances {
                if instance == &spec.instance_id {
                    if let Some(token) = q.tokens.get(id) {
                        token.store(true, Ordering::Release);
                    }
                }
            }
            if !q.closes.contains(&spec.instance_id) {
                q.closes.push_back(spec.instance_id.clone());
            }
            let mut rejected = Vec::new();
            let mut keep = VecDeque::new();
            for p in q.requests.drain(..) {
                if p.request.instance_id == spec.instance_id {
                    rejected.push(p);
                } else {
                    keep.push_back(p);
                }
            }
            q.requests = keep;
            drop(q);
            for p in rejected {
                let error = queue()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .failures
                    .get(&spec.instance_id)
                    .map(|(_, _, e)| e.clone())
                    .unwrap_or_else(|| "studio_closed".into());
                finish(p, Err(error));
            }
            SignalToUI::set_ui_signal();
        }
        self.view.visible = false;
        self.view.redraw(cx);
    }
    fn start(&mut self, cx: &mut Cx) -> Result<(), String> {
        let spec = self.spec.as_ref().ok_or("studio_instance_not_found")?;
        if !granted(&spec.owner, &spec.dev_tag) {
            return Err("studio_grant_expired".into());
        }
        if !spec.jail.is_dir() {
            return Err("studio_storage_missing".into());
        }
        crate::glance_card::ensure_vocabulary(cx);
        let card = self.view.splash(cx, ids!(card));
        if let Some(mut s) = card.borrow_mut() {
            s.set_allow_net(false);
            s.set_debug_name(&format!("studio/{}/main.splash", spec.instance_id));
        }
        card.set_sandbox_dir(cx, Some(spec.jail.clone()));
        card.set_storage_quota(cx, Some(spec.storage_quota));
        // Fail closed on the device and carry no host identity, as a seated
        // contained app does (`glance_card::seat`).
        card.set_device_consent(cx, true);
        card.set_host_tag(cx, None);
        card.set_host_caps(cx, Vec::new());
        card.set_host_prompts(cx, false);
        // The admitted budget is a declaration since the ruling of 8 October
        // 2026; the jail, quota and memory cap are what is enforced.
        card.set_policy(cx, Some(Vec::new()), Some(spec.instruction_budget));
        card.set_memory_bytes(cx, Some(spec.memory_bytes as usize));
        card.set_text(cx, &spec.source);
        let mut children = 0;
        if let Some(s) = card.borrow() {
            s.children(&mut |_, _| children += 1);
        }
        if children == 0 {
            return Err(format!("studio_eval_failed: {}/main.splash produced no root content", spec.instance_id));
        }
        self.timer = cx.start_interval(0.08);
        Ok(())
    }
    fn dispatch(&mut self, cx: &mut Cx, event: &Event) {
        let card = self.view.splash(cx, ids!(card));
        let vm = card
            .borrow()
            .and_then(|s| cx.script_ref_vm_id(&s.view.source));
        if let Some(vm) = vm {
            widget_async::with_isolate(cx, vm, |cx| {
                self.view.handle_event(cx, event, &mut Scope::empty())
            });
        }
    }
    fn instrument(&self, cx: &mut Cx) -> Value {
        let tree = WidgetTree::default();
        let card = self.view.widget(cx, ids!(card));
        tree.set_root_widget(card.clone());
        let mut names: HashMap<String, usize> = HashMap::new();
        let widgets: Vec<Value> = tree
            .snapshot(cx)
            .into_iter()
            .map(|w| {
                let occurrence = names.entry(w.id.clone()).or_default();
                let selector = format!("{}@{}", w.id, *occurrence);
                *occurrence += 1;
                // Splash.text() is source code, not painted UI text. Never
                // let source literals satisfy a functional visible-text check.
                let text = if w.widget_type == "Splash" {
                    None
                } else {
                    w.text
                };
                let value = if w.widget_type == "Splash" {
                    None
                } else {
                    w.value
                };
                json!({
            "id":w.id,"selector":selector,"type":w.widget_type,"rect":[w.x,w.y,w.width,w.height],
            "visible":w.visible,"enabled":w.enabled,"text":text,"value":value,
            "checked":w.checked,"selected":w.selected})
            })
            .collect();
        let mut geometry: Value =
            serde_json::from_str(&tree.geometry_json(cx)).unwrap_or(Value::Null);
        // ScrollYView is a View template, so its Rust type alone cannot tell
        // intentional viewport clipping from a layout bug. Read its inherited
        // scroll_bars setting in its own VM without evaluating any app code.
        let rows = tree.flat_tree(cx);
        let mut name_counts = HashMap::<String, usize>::new();
        for row in &rows {
            *name_counts.entry(row.name.clone()).or_default() += 1;
        }
        let vm = card
            .borrow::<Splash>()
            .and_then(|s| cx.script_ref_vm_id(&s.view.source));
        let mut scroll_y = Vec::new();
        if let Some(vm) = vm {
            widget_async::with_isolate(cx, vm, |cx| {
                cx.with_vm(|vm| {
                    for row in &rows {
                        if name_counts.get(&row.name) != Some(&1) {
                            continue;
                        }
                        let source = tree.widget(WidgetUid(row.uid)).script_source();
                        if source == ScriptObject::ZERO {
                            continue;
                        }
                        if let Some(bars) = vm
                            .bx
                            .heap
                            .value(source, id!(scroll_bars).into(), NoTrap)
                            .as_object()
                        {
                            if vm
                                .bx
                                .heap
                                .value(bars, id!(show_scroll_y).into(), NoTrap)
                                .as_bool()
                                == Some(true)
                            {
                                scroll_y.push(row.name.clone());
                            }
                        }
                    }
                })
            });
        }
        if let Some(nodes) = geometry["widgets"].as_array_mut() {
            for node in nodes {
                let is_scroll = node["id"]
                    .as_str()
                    .is_some_and(|name| scroll_y.iter().any(|id| id == name));
                if is_scroll {
                    node["scroll_y"] = json!(true);
                }
            }
        }
        let checks = check_geometry(&geometry);
        json!({"widgets":widgets,"geometry":geometry,"tree":tree.compact_dump(cx),"checks":checks})
    }
    fn target(&self, cx: &mut Cx, id: &str) -> Result<DVec2, String> {
        let snapshot = self.instrument(cx);
        let widgets = snapshot["widgets"]
            .as_array()
            .ok_or("studio_snapshot_failed")?;
        let candidate = select_widget(widgets, id)?;
        let r = candidate["rect"].as_array().ok_or("studio_widget_rect")?;
        let n = |i: usize| r[i].as_f64().unwrap_or(0.0);
        if n(2) <= 0.0 || n(3) <= 0.0 {
            return Err("studio_widget_not_drawn".into());
        }
        let abs = dvec2(n(0) + n(2) * 0.5, n(1) + n(3) * 0.5);
        Ok(abs)
    }
    fn redraw_after_input(&mut self, cx: &mut Cx) {
        // A redraw request only queues Draw; the old cache can still be clean.
        // Inspect must wait until the changed widget state has been recorded.
        self.input_draw_pending = true;
        self.view.redraw(cx);
    }
    fn scroll(&mut self, cx: &mut Cx, id: &str, dy: f64) -> Result<(), String> {
        let abs = self.target(cx, id)?;
        let window_id = cx.windows.id_iter().next().ok_or("studio_no_window")?;
        self.dispatch(
            cx,
            &Event::Scroll(ScrollEvent {
                window_id,
                abs,
                scroll: dvec2(0., dy),
                modifiers: Default::default(),
                is_mouse: false,
                time: cx.seconds_since_app_start(),
                handled_x: Default::default(),
                handled_y: Default::default(),
                phase: Default::default(),
            }),
        );
        self.redraw_after_input(cx);
        Ok(())
    }
    fn tap(&mut self, cx: &mut Cx, id: &str) -> Result<(), String> {
        let abs = self.target(cx, id)?;
        let window_id = cx.windows.id_iter().next().ok_or("studio_no_window")?;
        // Same TouchUpdate hit/handler path as physical input, scoped to this
        // visible app. Keep synthetic pointer capture out of the live fingers.
        let fingers = std::mem::take(&mut cx.fingers);
        for (offset, state) in [(0.0, TouchState::Start), (0.03, TouchState::Stop)] {
            let time = cx.seconds_since_app_start() + offset;
            self.dispatch(
                cx,
                &Event::TouchUpdate(TouchUpdateEvent {
                    time,
                    window_id,
                    modifiers: Default::default(),
                    touches: vec![TouchPoint {
                        uid: 0x53545544,
                        state,
                        abs,
                        time,
                        rotation_angle: 0.0,
                        force: 1.0,
                        radius: dvec2(1., 1.),
                        handled: Default::default(),
                        sweep_lock: Default::default(),
                    }],
                }),
            );
        }
        cx.fingers = fingers;
        self.redraw_after_input(cx);
        Ok(())
    }
    fn pump(&mut self, cx: &mut Cx) {
        let Some(spec) = self.spec.as_ref() else {
            return;
        };
        if self.pending.is_none() {
            let mut q = queue().lock().unwrap_or_else(|e| e.into_inner());
            if let Some(i) = q
                .requests
                .iter()
                .position(|p| p.request.instance_id == spec.instance_id)
            {
                self.pending = q.requests.remove(i);
            }
        }
        let Some(p) = self.pending.take() else { return };
        if let Err(e) = valid(&p) {
            self.text_next = None;
            finish(p, Err(e));
            return;
        }
        if self.draws < 2 {
            self.pending = Some(p);
            self.view.redraw(cx);
            return;
        }
        if let Some(text) = self.text_next.take() {
            self.dispatch(
                cx,
                &Event::TextInput(TextInputEvent {
                    input: text,
                    replace_last: false,
                    was_paste: true,
                    ..Default::default()
                }),
            );
            self.redraw_after_input(cx);
            finish(p, Ok(json!({"ok":true})));
            return;
        }
        match &p.request.action {
            Action::Ready => finish(p, Ok(json!({"instance_id":spec.instance_id,"ready":true}))),
            Action::Input {
                widget_id,
                kind,
                text,
                delta_y,
            } => {
                if kind != "tap" && kind != "text" && kind != "scroll" {
                    finish(p, Err("studio_input_kind".into()));
                    return;
                }
                if kind == "text" && text.as_ref().is_none_or(|t| t.len() > 4096) {
                    finish(p, Err("studio_input_text".into()));
                    return;
                }
                let follow = (kind == "text").then(|| text.clone().unwrap());
                let result = if kind == "scroll" {
                    match delta_y {
                        Some(dy) if dy.is_finite() && dy.abs() <= 2000.0 => {
                            self.scroll(cx, widget_id, *dy)
                        }
                        _ => Err("studio_input_scroll".into()),
                    }
                } else {
                    self.tap(cx, widget_id)
                };
                match result {
                    Err(e) => finish(p, Err(e)),
                    Ok(()) => {
                        if let Some(text) = follow {
                            self.text_next = Some(text);
                            self.pending = Some(p);
                        } else {
                            finish(p, Ok(json!({"ok":true})));
                        }
                    }
                }
            }
            Action::Inspect { .. } => {
                if self.input_draw_pending {
                    // Keep the original request/deadline. The UI remains free
                    // to draw; valid() above cancels a stalled/background app.
                    self.pending = Some(p);
                    self.view.redraw(cx);
                    return;
                }
                let snapshot = self.instrument(cx);
                if snapshot.to_string().len() > 1024 * 1024 {
                    finish(p, Err("studio_snapshot_too_large".into()));
                    return;
                }
                let frozen = self
                    .view
                    .view(cx, ids!(capture))
                    .borrow_mut()
                    .and_then(|mut view| view.take_texture_snapshot(cx));
                if let Some(frozen) = frozen {
                    match frozen
                        .texture()
                        .read_back(cx, ReadbackRequest { next_render: false })
                    {
                        Ok(ticket) => CAPTURES.with(|c| {
                            c.borrow_mut().push(Capture {
                                pending: p,
                                snapshot,
                                frozen,
                                ticket,
                            })
                        }),
                        Err(e) => finish(p, Err(format!("studio_readback: {e}"))),
                    }
                    self.view.redraw(cx);
                } else {
                    self.pending = Some(p);
                    self.view.redraw(cx);
                }
            }
            Action::Close => {
                finish_after_cleanup(p, || self.stop(cx));
            }
        }
    }
}
impl Widget for StudioApp {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if !self.started {
            self.started = true;
            if let Err(e) = self.start(cx) {
                let id = self
                    .spec
                    .as_ref()
                    .map(|s| s.instance_id.clone())
                    .unwrap_or_default();
                error!("studio app {id}: {e}");
                if let Some(spec) = self.spec.as_ref() {
                    let mut q = queue().lock().unwrap_or_else(|e| e.into_inner());
                    if q.failures.len() >= MAX_REQUESTS {
                        if let Some(old) = q.failures.keys().next().cloned() {
                            q.failures.remove(&old);
                        }
                    }
                    q.failures
                        .insert(id, (spec.owner.clone(), spec.dev_tag.clone(), e));
                }
                self.stop(cx);
                return;
            }
        }
        if self.spec.is_none() {
            return;
        }
        super::refuse_host_requests(cx, &self.view.splash(cx, ids!(card)));
        self.dispatch(cx, event);
        self.pump(cx);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let card = self.view.splash(cx, ids!(card));
        let vm = card
            .borrow()
            .and_then(|s| cx.script_ref_vm_id(&s.view.source));
        let result = if let Some(vm) = vm {
            widget_async::with_isolate(cx, vm, |cx| self.view.draw_walk(cx, scope, walk))
        } else {
            self.view.draw_walk(cx, scope, walk)
        };
        if result.is_done() {
            self.draws += 1;
            self.input_draw_pending = false;
        }
        result
    }
}
fn select_widget<'a>(widgets: &'a [Value], id: &str) -> Result<&'a Value, String> {
    let mut matches = widgets.iter().filter(|w| {
        (w["id"].as_str() == Some(id) || w["selector"].as_str() == Some(id))
            && w["visible"] == true
            && w["enabled"] == true
    });
    let first = matches.next().ok_or("studio_widget_missing_or_ambiguous")?;
    if matches.next().is_some() {
        return Err("studio_widget_missing_or_ambiguous".into());
    }
    Ok(first)
}
/// Pure measured checks on Makepad's actual ink/clip rectangles; no model score.
fn check_geometry(geometry: &Value) -> Value {
    let mut findings = Vec::new();
    if geometry["widgets"]
        .as_array()
        .is_none_or(|nodes| nodes.is_empty())
    {
        findings.push(json!({"code":"empty_app","severity":"error"}));
    }
    if let Some(nodes) = geometry["widgets"].as_array() {
        let by_index: HashMap<i64, &Value> = nodes
            .iter()
            .filter_map(|node| node["i"].as_i64().map(|i| (i, node)))
            .collect();
        for node in nodes {
            let n = |key: &str| node[key].as_f64().unwrap_or(0.0);
            let kind = node["kind"].as_str().unwrap_or("");
            if (kind.contains("Label") || kind.contains("TextInput"))
                && node["text"].as_str().is_some_and(|s| !s.is_empty())
                && (n("cw") + 1. < n("w") || n("ch") + 1. < n("h"))
                && !scroll_explains_clip(node, &by_index)
            {
                findings.push(json!({"code":"text_clipped","severity":"error","id":node["id"]}));
            }
            if kind.contains("Button") && (n("w") < 44. || n("h") < 44.) {
                findings
                    .push(json!({"code":"small_tap_target","severity":"warning","id":node["id"]}));
            }
        }
    }
    json!({"pass":!findings.iter().any(|f|f["severity"]=="error"),"findings":findings,"instrument":"makepad_widget_tree"})
}
fn scroll_explains_clip(node: &Value, by_index: &HashMap<i64, &Value>) -> bool {
    let n = |key: &str| node[key].as_f64().unwrap_or(0.0);
    // A vertical scroll viewport never excuses horizontal truncation.
    if n("cw") + 1.0 < n("w") && n("ch") > 0.0 {
        return false;
    }
    let mut at = node["parent"].as_i64();
    for _ in 0..64 {
        let Some(parent) = at.and_then(|i| by_index.get(&i).copied()) else {
            return false;
        };
        if parent["scroll_y"] == true {
            let top = parent["cy"].as_f64().unwrap_or(0.0);
            let bottom = top + parent["ch"].as_f64().unwrap_or(0.0);
            let expected_top = n("y").max(top);
            let expected_bottom = (n("y") + n("h")).min(bottom);
            let expected_height = (expected_bottom - expected_top).max(0.0);
            if (expected_height - n("ch")).abs() <= 1.0
                && (expected_height == 0.0 || (expected_top - n("cy")).abs() <= 1.0)
            {
                return true;
            }
        }
        at = parent["parent"].as_i64();
    }
    false
}
pub fn readback(cx: &mut Cx, result: TextureReadback) -> Option<TextureReadback> {
    let capture = CAPTURES.with(|all| {
        let mut all = all.borrow_mut();
        all.iter()
            .position(|c| c.ticket == result.ticket)
            .map(|i| all.remove(i))
    });
    let Some(capture) = capture else {
        return Some(result);
    };
    let bytes = match valid(&capture.pending).and_then(|_| rgba(&result, false)) {
        Ok(bytes) => bytes,
        Err(e) => {
            finish(capture.pending, Err(e));
            return None;
        }
    };
    let reply = capture.pending.request.reply.clone();
    let id = capture.pending.request.id.clone();
    // `frozen` pins the exact app framebuffer until readback completion above.
    drop(capture.frozen);
    let task=cx.task_pool().submit(Lane::Heavy,move||{
        let mut p=capture.pending;
        let outcome=(||{
            valid(&p)?;
            let png=Cx::encode_rgba_as_png(result.width as u32,result.height as u32,&bytes).map_err(|e|format!("studio_png: {e:?}"))?;
            if png.len()>MAX_PNG{return Err("studio_png_too_large".into());}
            valid(&p)?;
            let Action::Inspect{output}=&mut p.request.action else{return Err("studio_capture_kind".into());};
            output.write_all(&png).and_then(|_|output.flush()).map_err(|e|e.to_string())?;
            valid(&p)?;
            Ok(json!({"instance_id":p.request.instance_id,"width":result.width,"height":result.height,"settled":false,"snapshot":capture.snapshot}))
        })();finish(p,outcome);
    });
    match task {
        Ok(task) => task.detach(),
        Err(e) => {
            let _ = reply.send(Err(format!("studio_worker: {e}")));
            let mut q = queue().lock().unwrap_or_else(|e| e.into_inner());
            q.tokens.remove(&id);
            q.request_instances.remove(&id);
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    /// Records the actual StudioApp widget through its cached-view draw path.
    /// This catches incompatible View flags and stack imbalance; GPU pixels
    /// remain the separate device acceptance test.
    #[test]
    fn studio_widget_records_a_cached_frame_without_background_or_stack_conflicts() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let root = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            super::script_mod(vm);
            let value = script_eval!(vm,{use mod.widgets.* StudioApp{}});
            WidgetRef::script_from_value(vm, value)
        });
        root.splash(&mut cx, ids!(card))
            .set_text(&mut cx, "Label{text: \"Studio render regression\"}");
        let cache = root.view(&mut cx, ids!(capture));
        assert!(cache.borrow().unwrap().texture_caching);
        assert!(!cache.borrow().unwrap().show_bg);
        record_frame(&mut cx, &root);
    }
    #[test]
    fn input_redraw_cannot_reuse_the_previous_completed_frame() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let root = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            super::script_mod(vm);
            let value = script_eval!(vm,{use mod.widgets.* StudioApp{}});
            WidgetRef::script_from_value(vm, value)
        });
        root.splash(&mut cx, ids!(card))
            .set_text(&mut cx, "Label{text: \"Before input\"}");
        record_frame(&mut cx, &root);
        let previous_draws = root.borrow::<StudioApp>().unwrap().draws;
        {
            let mut app = root.borrow_mut::<StudioApp>().unwrap();
            // This is the production invalidation used by tap/scroll and the
            // second (text delivery) event. A queued redraw is not a frame.
            app.redraw_after_input(&mut cx);
            assert!(app.input_draw_pending);
            assert_eq!(app.draws, previous_draws);
            app.redraw_after_input(&mut cx);
            assert!(app.input_draw_pending);
        }
        record_frame(&mut cx, &root);
        let app = root.borrow::<StudioApp>().unwrap();
        assert!(app.draws > previous_draws);
        assert!(!app.input_draw_pending);
        // GPU completion is still checked separately by take_texture_snapshot;
        // this CPU test does not claim the recorded frame has been painted.
    }
    #[test]
    fn close_acknowledgement_is_sent_only_after_cleanup() {
        let (tx, rx) = std::sync::mpsc::channel();
        let pending = Pending {
            request: Request {
                id: format!("close-order-{}", uuid::Uuid::new_v4()),
                instance_id: "test-instance".into(),
                owner: "system".into(),
                dev_tag: DevTag {
                    profile_id: "test-only".into(),
                    since: 0,
                },
                action: Action::Close,
                reply: tx,
            },
            started: Instant::now(),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        let mut disposed = false;
        finish_after_cleanup(pending, || {
            assert!(matches!(
                rx.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            ));
            disposed = true;
        });
        assert!(disposed);
        assert_eq!(rx.try_recv().unwrap().unwrap(), json!({"closed":true}));
    }
    fn record_frame(cx: &mut Cx, root: &WidgetRef) {
        let pass = DrawPass::new(cx);
        pass.set_size(cx, dvec2(390., 600.));
        let mut list = DrawList2d::new(cx);
        let event = DrawEvent::default();
        let mut draw = CxDraw::new(cx, &event);
        let mut cx = Cx2d::new(&mut draw);
        cx.begin_pass(&pass, Some(1.));
        list.begin_always(&mut cx);
        cx.begin_root_turtle(dvec2(390., 600.), Layout::flow_overlay());
        let mark = cx.unwind_mark();
        root.draw_walk_all(&mut cx, &mut Scope::empty(), Walk::fixed(390., 600.));
        assert!(cx.unwind_mark().is_balanced_with(&mark));
        cx.end_pass_sized_turtle();
        list.end(&mut cx);
        cx.end_pass(&pass);
    }
    #[test]
    fn production_shutdown_parks_the_vm_reclaims_scratch_and_allows_next_app_draw() {
        use makepad_app_module::{
            AppModule, InstanceHandles, InstanceScope, ModuleWindows, ReplySink, Viewport,
        };
        let instance = format!("shutdown-test-{}", uuid::Uuid::new_v4());
        let jail = std::env::temp_dir().join(&instance);
        std::fs::create_dir(&jail).unwrap();
        let spec = OpenSpec {
            instance_id: instance.clone(),
            app_id: None,
            owner: "system".into(),
            dev_tag: DevTag {
                profile_id: "test-only".into(),
                since: 0,
            },
            source: String::new(),
            jail: jail.clone(),
            persistent: false,
            title: "test".into(),
            storage_quota: 0,
            instruction_budget: 5_000_000,
            memory_bytes: 16 * 1024 * 1024,
        };
        // No grant is minted: the test seats an internal spec solely to exercise
        // the production factory/shutdown boundary and disposal of its jail.
        queue().lock().unwrap().specs.insert(instance.clone(), spec);
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let storage = cx.storage(&instance);
        let parts = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            let module = &super::super::module::STUDIO_MODULE;
            module.register(vm);
            let open = module
                .open_schema()
                .validate(&json!({"instance_id":instance}).to_string(), &[])
                .unwrap();
            module.create(
                vm,
                open,
                InstanceHandles {
                    scope: InstanceScope::new(1, 1),
                    storage,
                    viewport: Viewport {
                        size: dvec2(390., 600.),
                    },
                    replies: ReplySink::pair().0,
                    windows: ModuleWindows::new(false),
                },
            )
        });
        let card = parts.root.splash(&mut cx, ids!(card));
        card.set_sandbox_dir(&mut cx, Some(jail.clone()));
        card.set_text(&mut cx, "Label{text: \"Before shutdown\"}");
        record_frame(&mut cx, &parts.root);
        let shutdown = parts.shutdown;
        // Invoke the actual FnOnce while Cx's VM is taken, exactly the boundary
        // that used to panic. No ModuleHost catch can turn a panic into success.
        cx.with_vm(|vm| shutdown(vm));
        assert!(!jail.exists());
        assert!(!ROOTS.with(|roots| roots.borrow().contains_key(&instance)));
        assert!(!queue().lock().unwrap().specs.contains_key(&instance));
        assert!(parts.root.borrow::<StudioApp>().unwrap().spec.is_none());
        let next = cx.with_vm(|vm| {
            let value = script_eval!(vm,{use mod.widgets.* StudioApp{}});
            WidgetRef::script_from_value(vm, value)
        });
        next.splash(&mut cx, ids!(card))
            .set_text(&mut cx, "Label{text: \"After shutdown\"}");
        record_frame(&mut cx, &next);
        queue().lock().unwrap().closes.retain(|id| id != &instance);
    }
    #[test]
    fn scoped_selectors_refuse_ambiguous_hidden_and_disabled_targets() {
        let rows = vec![
            json!({"id":"complete","selector":"complete@0","visible":true,"enabled":true}),
            json!({"id":"complete","selector":"complete@1","visible":true,"enabled":true}),
            json!({"id":"delete","selector":"delete@0","visible":false,"enabled":true}),
            json!({"id":"blocked","selector":"blocked@0","visible":true,"enabled":false}),
        ];
        assert!(select_widget(&rows, "complete").is_err());
        assert_eq!(select_widget(&rows, "complete@1").unwrap(), &rows[1]);
        assert!(select_widget(&rows, "delete@0").is_err());
        assert!(select_widget(&rows, "blocked@0").is_err());
        assert!(select_widget(&rows, "outside_instance").is_err());
    }
    #[test]
    fn missing_geometry_never_passes_inspection() {
        assert_eq!(check_geometry(&json!({"widgets":[]}))["pass"], false);
    }
    #[test]
    fn scroll_viewport_clipping_is_distinct_from_a_short_row_or_horizontal_overflow() {
        let mut report = json!({"widgets":[
            {"i":0,"parent":-1,"id":"list","kind":"View","scroll_y":true,"cy":0,"ch":100},
            {"i":1,"parent":0,"id":"row","kind":"Label","text":"task","y":90,"w":100,"h":20,"cy":90,"cw":100,"ch":10}]});
        assert_eq!(check_geometry(&report)["pass"], true);
        report["widgets"][1]["ch"] = json!(5);
        assert_eq!(check_geometry(&report)["pass"], false);
        report["widgets"][1]["ch"] = json!(10);
        report["widgets"][1]["cw"] = json!(50);
        assert_eq!(check_geometry(&report)["pass"], false);
    }
    #[test]
    fn instrument_checks_report_real_clipping_and_small_controls() {
        let report = check_geometry(&json!({"widgets":[
            {"id":"title","kind":"Label","text":"Tasks","w":100,"h":20,"cw":40,"ch":20},
            {"id":"add","kind":"Button","w":30,"h":30,"cw":30,"ch":30}]}));
        assert_eq!(report["pass"], false);
        assert_eq!(report["findings"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn empty_or_complete_text_is_not_clipped() {
        assert_eq!(
            check_geometry(
                &json!({"widgets":[{"id":"title","kind":"Label","text":"Tasks","w":100,"h":20,"cw":100,"ch":20}]})
            )["pass"],
            true
        );
    }
}
