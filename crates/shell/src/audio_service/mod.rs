//! Foreground, app-scoped short recording and playback. No ASR/TTS, remote
//! URLs, background playback, system-loopback capture or raw host paths.
use crossbeam_queue::ArrayQueue;
use makepad_widgets::{
    makepad_platform::{
        audio::{AudioDeviceId, AudioInputLease},
        permission::{Permission, PermissionStatus},
        thread::ThreadOptions,
        SignalToUI,
    },
    splash_host,
    splash_storage::{self, StorageAccess},
    Cx, CxMediaApi, Event, Timer,
};
use octosense_appstore::services::{
    self, AgentAccess, HostApiMethod, HostService, Replier, ServiceCall, ServiceHost,
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};
mod codec;
#[cfg(test)]
mod tests;

const MAX_SESSIONS: usize = 16;
const CHUNK: usize = 1024;
const RETAIN: Duration = Duration::from_secs(60);
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Record,
    Play,
}
static RECORD_WORKER: AtomicBool = AtomicBool::new(false);
static PLAY_WORKER: AtomicBool = AtomicBool::new(false);
struct WorkerSlot(Kind);
impl WorkerSlot {
    fn acquire(kind: Kind) -> Result<Self, String> {
        let flag = if kind == Kind::Record {
            &RECORD_WORKER
        } else {
            &PLAY_WORKER
        };
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self(kind))
            .map_err(|_| "busy: The previous audio worker is still finishing".into())
    }
}
impl Drop for WorkerSlot {
    fn drop(&mut self) {
        if self.0 == Kind::Record {
            &RECORD_WORKER
        } else {
            &PLAY_WORKER
        }
        .store(false, Ordering::Release);
    }
}
#[derive(Debug, PartialEq)]
enum Operation {
    Start {
        kind: Kind,
        path: String,
        millis: u64,
    },
    Status(String),
    Stop(String),
    Cancel(String),
}
fn parse(family: &str, method: &str, args: &Value) -> Result<Operation, String> {
    let object = args
        .as_object()
        .ok_or("invalid_arguments: Expected an object")?;
    let start = matches!(
        (family, method),
        ("microphone", "record_start") | ("audio", "play")
    );
    let allowed: &[&str] = if start && family == "microphone" {
        &["path", "max_duration_ms"]
    } else if start {
        &["path"]
    } else {
        &["session"]
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("invalid_arguments: Unknown audio argument".into());
    }
    if start {
        let path = args["path"]
            .as_str()
            .filter(|p| !p.is_empty() && p.len() <= 2048)
            .ok_or("invalid_arguments: path must name an app file")?;
        let root = std::path::Path::new("/");
        if splash_storage::resolve_jailed(root, path)? == root {
            return Err("invalid_arguments: path must name a file".into());
        }
        let millis = match args.get("max_duration_ms") {
            None => codec::MAX_RECORD_MS,
            Some(v) => v
                .as_u64()
                .filter(|n| (100..=codec::MAX_RECORD_MS).contains(n))
                .ok_or("invalid_arguments: max_duration_ms must be 100..30000")?,
        };
        return Ok(Operation::Start {
            kind: if family == "microphone" {
                Kind::Record
            } else {
                Kind::Play
            },
            path: path.into(),
            millis,
        });
    }
    let id = args["session"]
        .as_str()
        .filter(|s| s.len() == 36)
        .ok_or("invalid_arguments: session must be a host session id")?
        .to_owned();
    match (family, method) {
        ("microphone", "record_status") | ("audio", "status") => Ok(Operation::Status(id)),
        ("microphone", "record_stop") | ("audio", "stop") => Ok(Operation::Stop(id)),
        ("microphone", "record_cancel") => Ok(Operation::Cancel(id)),
        _ => Err("method_unavailable: Unknown audio method".into()),
    }
}
fn supported() -> bool {
    cfg!(any(target_os = "macos", target_os = "android"))
}
fn descriptor(name: &str, family: &str, description: &str, input: Value) -> HostApiMethod {
    HostApiMethod::new(
        name,
        1,
        family,
        description,
        input,
        json!({"type":"object","required":["session","status","path","error","frames","format"],"additionalProperties":false,"properties":{
            "session":{"type":"string","minLength":36,"maxLength":36},
            "status":{"enum":["preparing","checking_permission","waiting_device","starting","recording","playing","stopping","saved","stopped","completed","failed","cancelled"]},
            "path":{"type":["string","null"],"description":"App-relative output path only after recording has been saved"},
            "error":{"type":["string","null"]},"frames":{"type":"integer","minimum":0,"description":"Device frames received or rendered; not delivery or audibility confirmation"},
            "format":{"enum":["wav",null]}}}),
    )
    .with_platforms(&["macos", "android"])
    .with_agent_access(AgentAccess::ForegroundOnly)
}
fn session_schema() -> Value {
    json!({"type":"object","required":["session"],"additionalProperties":false,"properties":{"session":{"type":"string","minLength":36,"maxLength":36}}})
}
pub(crate) fn microphone_methods() -> Vec<HostApiMethod> {
    if !supported() {
        return vec![];
    }
    let mut methods=vec![descriptor("microphone.record_start","microphone","Start a foreground mono WAV recording in app storage after existing app and OS microphone consent; maximum 30 seconds",json!({"type":"object","required":["path"],"additionalProperties":false,"properties":{"path":{"type":"string","minLength":1,"maxLength":2048},"max_duration_ms":{"type":"integer","minimum":100,"maximum":30000}}}))];
    for (method, description) in [
        ("record_status", "Read this live app's recording state"),
        (
            "record_stop",
            "Stop and save the recording into the same app storage",
        ),
        ("record_cancel", "Stop and discard the recording"),
    ] {
        methods.push(descriptor(
            &format!("microphone.{method}"),
            "microphone",
            description,
            session_schema(),
        ));
    }
    methods
}
pub fn register() {
    services::register_host_service(Box::new(AudioService));
}
struct AudioService;
impl HostService for AudioService {
    fn family(&self) -> &'static str {
        "audio"
    }
    fn api_methods(&self) -> Vec<HostApiMethod> {
        if !supported() {
            return vec![];
        }
        vec![
        descriptor("audio.play","audio","Play a bounded WAV, MP3, FLAC or Ogg file from this app's storage while the app stays in the foreground",json!({"type":"object","required":["path"],"additionalProperties":false,"properties":{"path":{"type":"string","minLength":1,"maxLength":2048}}})),
        descriptor("audio.status","audio","Read this live app's playback state",session_schema()),descriptor("audio.stop","audio","Stop this app's playback",session_schema())]
    }
    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        self::call(call, reply);
    }
}
struct Work {
    call: ServiceCall,
    reply: Replier,
    operation: Operation,
}
fn queue() -> &'static ArrayQueue<Work> {
    static QUEUE: OnceLock<ArrayQueue<Work>> = OnceLock::new();
    QUEUE.get_or_init(|| ArrayQueue::new(16))
}
pub(crate) fn call(call: ServiceCall, reply: Replier) {
    let result = (|| {
        if !supported() {
            return Err(
                "unsupported_platform: Native audio sessions currently support macOS and Android"
                    .into(),
            );
        }
        if !call.may_prompt || call.from_sheet {
            return Err("foreground_required: Open the installed app to use audio".into());
        }
        parse(
            call.service.split('.').next().unwrap_or(""),
            call.method(),
            &call.args,
        )
    })();
    match result {
        Ok(operation) => {
            if let Err(work) = queue().push(Work {
                call,
                reply,
                operation,
            }) {
                work.reply.send(Err("busy: Too many audio requests".into()));
            } else {
                SignalToUI::set_ui_signal();
            }
        }
        Err(error) => reply.send(Err(error)),
    }
}
#[derive(Clone, Copy)]
struct Chunk {
    rate: u32,
    len: usize,
    samples: [f32; CHUNK],
}
struct Shared {
    queue: ArrayQueue<Chunk>,
    stop: AtomicBool,
    cancel: AtomicBool,
    overflow: AtomicBool,
    invalid: AtomicBool,
    frames: AtomicU64,
    ended: AtomicBool,
}
impl Shared {
    fn new() -> Self {
        Self {
            queue: ArrayQueue::new(32),
            stop: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            overflow: AtomicBool::new(false),
            invalid: AtomicBool::new(false),
            frames: AtomicU64::new(0),
            ended: AtomicBool::new(false),
        }
    }
}
struct Prepared {
    manifest: Value,
    revision: Option<u64>,
    pcm: Option<Arc<makepad_audio_decode::DecodedAudio>>,
}
enum Update {
    Prepared(Result<Prepared, String>),
    Finished(Result<Option<splash_storage::PreparedStorageImport>, String>),
}
struct Session {
    id: String,
    app: String,
    heap: usize,
    host: std::path::PathBuf,
    kind: Kind,
    path: String,
    millis: u64,
    storage: StorageAccess,
    shared: Arc<Shared>,
    rx: Receiver<Update>,
    start_reply: Option<Replier>,
    revision: Option<u64>,
    permission: Option<i32>,
    lease: Option<AudioInputLease>,
    pcm: Option<Arc<makepad_audio_decode::DecodedAudio>>,
    state: &'static str,
    error: Option<String>,
    started: Instant,
    ended: Option<Instant>,
    device: Option<AudioDeviceId>,
    last_frames: u64,
    last_frame_at: Instant,
}
impl Session {
    fn snapshot(&self) -> Value {
        json!({"session":self.id,"status":self.state,"path":if self.state=="saved"{Some(&self.path)}else{None},"error":self.error,"frames":self.shared.frames.load(Ordering::Acquire),"format":if self.kind==Kind::Record{Some("wav")}else{None}})
    }
    fn owns(&self, app: &str, heap: usize) -> bool {
        self.app == app && self.heap == heap
    }
    fn active(&self) -> bool {
        self.ended.is_none()
    }
    fn cancel(&mut self, cx: &mut Cx) {
        if self.active() {
            self.finish(
                cx,
                "cancelled",
                Some("cancelled: Recording discarded".into()),
            );
        }
    }
    fn stop(&mut self, cx: &mut Cx) {
        if !self.active() {
            return;
        }
        if self.device.is_none() {
            self.finish(
                cx,
                "cancelled",
                Some("cancelled: Audio stopped before device startup".into()),
            );
        } else {
            self.shared.stop.store(true, Ordering::Release);
            self.state = "stopping";
        }
    }
    fn finish(&mut self, cx: &mut Cx, state: &'static str, error: Option<String>) {
        self.shared.stop.store(true, Ordering::Release);
        if state != "saved" && state != "completed" {
            self.shared.cancel.store(true, Ordering::Release);
        }
        if self.device.take().is_some() {
            if self.kind == Kind::Record {
                cx.use_audio_inputs(&[]);
            } else {
                cx.use_audio_outputs(&[]);
                // Replacing the callback on the UI thread releases its PCM
                // only while the session still owns the other Arc.
                cx.audio_output(0, |_, buffer| buffer.data.fill(0.0));
            }
        }
        self.lease = None;
        self.pcm = None;
        self.state = state;
        self.error = error;
        self.ended = Some(Instant::now());
        if let Some(reply) = self.start_reply.take() {
            reply
                .send(Err(self.error.clone().unwrap_or_else(|| {
                    format!("cancelled: Audio session {state}")
                })));
        }
    }
}
struct State {
    sessions: HashMap<String, Session>,
    foreground: Option<String>,
    background: bool,
    window_unfocused: bool,
    input: Option<AudioDeviceId>,
    output: Option<AudioDeviceId>,
    timer: Timer,
}
impl Default for State {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
            foreground: None,
            background: false,
            // A desktop launch may start behind another application's window.
            // Only the native focus event can authorize this window initially.
            window_unfocused: cfg!(target_os = "macos"),
            input: None,
            output: None,
            timer: Timer::default(),
        }
    }
}
thread_local! {static STATE:RefCell<State>=RefCell::new(State::default());}
/// Set by the shell from its focused client or expanded Glance owner. It is not
/// exposed as a script method, and an unknown/preview identity fails closed.
pub fn set_foreground_app(app: Option<String>) {
    STATE.with(|state| state.borrow_mut().foreground = app);
}
fn live(session: &Session, foreground: Option<&str>, background: bool) -> bool {
    !background
        && session.start_reply.as_ref().is_none_or(Replier::is_pending)
        && foreground == Some(session.app.as_str())
        && splash_host::surface_is_foreground(session.heap, &session.app)
        && splash_storage::storage_for_heap(session.heap, &session.app)
            .is_some_and(|storage| storage.same_scope(&session.storage))
        && session.revision.is_none_or(|revision| {
            crate::platform_services::microphone_consent_current(
                &session.host,
                &session.app,
                revision,
            )
        })
}
fn admission(app: &str, host: &std::path::Path) -> Result<Value, String> {
    let loaded = crate::host_tools::script_apps::admitted_host(app, host)
        .map_err(|_| "permission_denied: App is no longer admitted")?;
    if !loaded.manifest["requires"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "host-api-v1"))
    {
        return Err(
            "host_requirement_missing: Declare host-api-v1 for native audio sessions".into(),
        );
    }
    Ok(loaded.manifest)
}

fn run_worker(
    app: String,
    host: std::path::PathBuf,
    kind: Kind,
    millis: u64,
    worker_storage: splash_storage::StorageSnapshot,
    worker_path: String,
    worker_shared: Arc<Shared>,
    tx: mpsc::SyncSender<Update>,
    worker_slot: WorkerSlot,
) {
    let _worker_slot = worker_slot;
    let prepared = (|| {
        if worker_shared.cancel.load(Ordering::Acquire) {
            return Err("cancelled: App closed before audio preparation".into());
        }
        let manifest = admission(&app, &host)?;
        let revision = if kind == Kind::Record {
            Some(crate::platform_services::microphone_consent(&host, &app)?)
        } else {
            None
        };
        let pcm = if kind == Kind::Play {
            if worker_shared.cancel.load(Ordering::Acquire) {
                return Err("cancelled: App closed before reading audio".into());
            }
            Some(Arc::new(codec::decode(
                &worker_storage.read_bytes(&worker_path)?,
            )?))
        } else {
            None
        };
        Ok(Prepared {
            manifest,
            revision,
            pcm,
        })
    })();
    let manifest = prepared.as_ref().ok().map(|value| value.manifest.clone());
    let okay = prepared.is_ok();
    if tx.send(Update::Prepared(prepared)).is_err() {
        return;
    }
    SignalToUI::set_ui_signal();
    if !okay {
        return;
    }
    let mut recorder = codec::Recorder::default();
    let mut policy_check = Instant::now();
    let mut failure = None;
    loop {
        if worker_shared.cancel.load(Ordering::Acquire) {
            failure = Some("cancelled: Audio session ended without saving".into());
            break;
        }
        if worker_shared.overflow.load(Ordering::Acquire) {
            failure =
                Some("audio_overflow: Capture could not keep up; recording was discarded".into());
            break;
        }
        if policy_check.elapsed() >= Duration::from_secs(1) {
            if admission(&app, &host).ok().as_ref() != manifest.as_ref() {
                worker_shared.invalid.store(true, Ordering::Release);
                failure = Some("permission_denied: App admission changed".into());
                break;
            }
            policy_check = Instant::now();
        }
        while let Some(chunk) = worker_shared.queue.pop() {
            match recorder.push(
                chunk.rate,
                &chunk.samples[..chunk.len],
                codec::RATE * millis as usize / 1000,
            ) {
                Ok(true) => {
                    worker_shared.stop.store(true, Ordering::Release);
                    break;
                }
                Ok(false) => {}
                Err(error) => {
                    failure = Some(error);
                    worker_shared.stop.store(true, Ordering::Release);
                    break;
                }
            }
        }
        if worker_shared.stop.load(Ordering::Acquire) || worker_shared.ended.load(Ordering::Acquire)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = match failure {
        Some(error) => Err(error),
        None if kind == Kind::Record => recorder
            .finish()
            .and_then(|bytes| {
                if worker_shared.cancel.load(Ordering::Acquire) {
                    Err("cancelled: Recording was discarded before staging".into())
                } else {
                    worker_storage.prepare_import(&worker_path, &bytes)
                }
            })
            .map(Some),
        None => Ok(None),
    };
    let _ = tx.send(Update::Finished(result));
    SignalToUI::set_ui_signal();
}

fn start(cx: &mut Cx, state: &mut State, work: Work, kind: Kind, path: String, millis: u64) {
    let failed = work.reply.clone();
    let result = (|| {
        if state.background
            || state.window_unfocused
            || state.foreground.as_deref() != Some(work.call.app_id.as_str())
            || !splash_host::surface_is_foreground(work.reply.isolate_key(), &work.call.app_id)
        {
            return Err("foreground_required: Open this installed app in its own window".into());
        }
        state
            .sessions
            .retain(|_, session| session.ended.is_none_or(|ended| ended.elapsed() < RETAIN));
        if state.sessions.len() >= MAX_SESSIONS
            || state
                .sessions
                .values()
                .any(|session| session.active() && session.kind == kind)
        {
            return Err(
                "busy: Another audio session owns this device, or retained session limit reached"
                    .into(),
            );
        }
        let storage = splash_storage::storage_for_heap(work.reply.isolate_key(), &work.call.app_id)
            .ok_or("storage_unavailable: App storage is no longer live")?;
        storage.validate_path(&path)?;
        let worker_storage = storage.worker_snapshot();
        let worker_path = path.clone();
        let lease = if kind == Kind::Record {
            Some(
                AudioInputLease::try_acquire()
                    .ok_or("busy: The microphone is in use by voice input or another app")?,
            )
        } else {
            None
        };
        let id = uuid::Uuid::new_v4().to_string();
        let worker_slot = WorkerSlot::acquire(kind)?;
        let shared = Arc::new(Shared::new());
        let (tx, rx) = mpsc::sync_channel(2);
        let worker_shared = shared.clone();
        let app = work.call.app_id.clone();
        let host = work.call.host_dir.clone();
        let handle = cx
            .thread_spawner()
            .spawn_worker(
                ThreadOptions {
                    name: Some("octosense-audio".into()),
                    ..Default::default()
                },
                move || {
                    run_worker(
                        app,
                        host,
                        kind,
                        millis,
                        worker_storage,
                        worker_path,
                        worker_shared,
                        tx,
                        worker_slot,
                    )
                },
            )
            .map_err(|_| "worker_unavailable: Cannot start the bounded audio worker")?;
        handle.detach();
        state.sessions.insert(
            id.clone(),
            Session {
                id,
                app: work.call.app_id,
                heap: work.reply.isolate_key(),
                host: work.call.host_dir,
                kind,
                path,
                millis,
                storage,
                shared,
                rx,
                start_reply: Some(work.reply),
                revision: None,
                permission: None,
                lease,
                pcm: None,
                state: "preparing",
                error: None,
                started: Instant::now(),
                ended: None,
                device: None,
                last_frames: 0,
                last_frame_at: Instant::now(),
            },
        );
        Ok::<_, String>(())
    })();
    if let Err(error) = result {
        failed.send(Err(error));
    }
}

pub fn handle_event(cx: &mut Cx, event: &Event) {
    STATE.with(|state| maintain(cx, &mut state.borrow_mut(), event));
}
fn maintain(cx: &mut Cx, state: &mut State, event: &Event) {
    if let Event::AudioDevices(devices) = event {
        let usable = |id: &AudioDeviceId| {
            devices
                .descs
                .iter()
                .any(|desc| desc.device_id == *id && !desc.has_failed)
        };
        let input = devices.default_input().first().copied().filter(usable);
        let output = devices.default_output().first().copied().filter(usable);
        for session in state
            .sessions
            .values_mut()
            .filter(|session| session.active())
        {
            if session.device.is_some_and(|id| {
                Some(id)
                    != if session.kind == Kind::Record {
                        input
                    } else {
                        output
                    }
            }) {
                session.finish(
                    cx,
                    "failed",
                    Some("device_changed: Audio device disconnected or changed".into()),
                );
            }
        }
        state.input = input;
        state.output = output;
    }
    if matches!(event, Event::Pause | Event::Background) {
        state.background = true;
    } else if matches!(event, Event::Resume | Event::Foreground) {
        state.background = false;
    }
    if matches!(event, Event::WindowLostFocus(_)) {
        state.window_unfocused = true;
    } else if matches!(event, Event::WindowGotFocus(_)) {
        state.window_unfocused = false;
    }
    // A queued Cancel is authoritative before a ready worker result can
    // publish bytes. Process control requests before committing completion.
    while let Some(work) = queue().pop() {
        if !work.reply.is_pending() {
            continue;
        }
        match work.operation {
            Operation::Start {
                kind,
                ref path,
                millis,
            } => {
                let path = path.clone();
                start(cx, state, work, kind, path, millis);
            }
            Operation::Status(ref id) | Operation::Stop(ref id) | Operation::Cancel(ref id) => {
                let Some(session) = state
                    .sessions
                    .get_mut(id)
                    .filter(|s| s.owns(&work.call.app_id, work.reply.isolate_key()))
                else {
                    work.reply.send(Err(
                        "session_unavailable: No audio session belongs to this live app".into(),
                    ));
                    continue;
                };
                if matches!(work.operation, Operation::Cancel(_)) {
                    session.cancel(cx);
                } else if matches!(work.operation, Operation::Stop(_)) {
                    session.stop(cx);
                }
                work.reply.send(Ok(session.snapshot()));
            }
        }
    }
    let foreground = state.foreground.clone();
    let background = state.background || state.window_unfocused;
    let input = state.input;
    let output = state.output;
    for session in state
        .sessions
        .values_mut()
        .filter(|session| session.active())
    {
        if !live(session, foreground.as_deref(), background)
            || session.shared.invalid.load(Ordering::Acquire)
        {
            session.finish(
                cx,
                "cancelled",
                Some("cancelled: App left the foreground, closed, or lost its grant".into()),
            );
            continue;
        }
        if session.started.elapsed() > Duration::from_millis(session.millis + 10_000)
            && session.kind == Kind::Record
        {
            session.finish(
                cx,
                "failed",
                Some("timeout: Recording did not complete within its bounded lifetime".into()),
            );
            continue;
        }
        if session.started.elapsed() > Duration::from_secs(75) {
            session.finish(cx, "failed", Some("timeout: Audio session expired".into()));
            continue;
        }
        if let Event::PermissionResult(result) = event {
            if session.permission == Some(result.request_id) {
                session.permission = None;
                if result.permission == Permission::AudioInput
                    && result.status == PermissionStatus::Granted
                {
                    begin_capture(cx, session, input);
                } else {
                    session.finish(
                        cx,
                        "failed",
                        Some(
                            "authorization_required: Grant microphone access in the foreground app"
                                .into(),
                        ),
                    );
                }
            }
        }
        match session.rx.try_recv() {
            Ok(Update::Prepared(Ok(prepared))) => {
                session.revision = prepared.revision;
                session.pcm = prepared.pcm;
                if !live(session, foreground.as_deref(), background) {
                    session.finish(
                        cx,
                        "cancelled",
                        Some("cancelled: App or microphone consent changed".into()),
                    );
                } else if session.kind == Kind::Record {
                    session.permission = Some(cx.check_permission(Permission::AudioInput));
                    session.state = "checking_permission";
                } else {
                    begin_playback(cx, session, output);
                }
            }
            Ok(Update::Prepared(Err(error))) => session.finish(cx, "failed", Some(error)),
            Ok(Update::Finished(Ok(bytes))) => {
                if let Some(staged) = bytes {
                    match splash_storage::storage_for_heap(session.heap, &session.app)
                        .filter(|storage| storage.same_scope(&session.storage))
                        .ok_or_else(|| "storage_unavailable: App storage changed".to_string())
                        .and_then(|storage| storage.commit_import(staged))
                    {
                        Ok(()) => session.finish(cx, "saved", None),
                        Err(error) => session.finish(cx, "failed", Some(error)),
                    }
                } else {
                    session.finish(
                        cx,
                        if session.state == "stopping" {
                            "stopped"
                        } else {
                            "completed"
                        },
                        None,
                    );
                }
            }
            Ok(Update::Finished(Err(error))) => session.finish(cx, "failed", Some(error)),
            Err(TryRecvError::Disconnected) if session.state == "preparing" => session.finish(
                cx,
                "failed",
                Some("worker_unavailable: Audio preparation stopped".into()),
            ),
            _ => {}
        }
        let frames = session.shared.frames.load(Ordering::Acquire);
        if frames != session.last_frames {
            session.last_frames = frames;
            session.last_frame_at = Instant::now();
        }
        if session.active()
            && matches!(session.state, "recording" | "playing")
            && session.last_frame_at.elapsed() > Duration::from_secs(2)
        {
            session.finish(
                cx,
                "failed",
                Some("device_unavailable: Audio device stopped delivering frames".into()),
            );
        }
        if session.active()
            && session.state == "waiting_device"
            && matches!(event, Event::AudioDevices(_))
        {
            if session.kind == Kind::Record && input.is_some() {
                begin_capture(cx, session, input);
            } else if session.kind == Kind::Play && output.is_some() {
                begin_playback(cx, session, output);
            }
        }
        if session.active()
            && session.shared.frames.load(Ordering::Acquire) > 0
            && session.state == "starting"
        {
            session.state = if session.kind == Kind::Record {
                "recording"
            } else {
                "playing"
            };
        }
        if session.active()
            && matches!(session.state, "starting" | "waiting_device")
            && session.started.elapsed() > Duration::from_secs(5)
        {
            session.finish(
                cx,
                "failed",
                Some("device_unavailable: No audio frames arrived".into()),
            );
        }
    }
    // Late worker completion after cancellation may contain decoded PCM.
    // Drain only the bounded pair of updates, dropping that payload on the UI.
    for session in state
        .sessions
        .values_mut()
        .filter(|session| !session.active())
    {
        for _ in 0..2 {
            if session.rx.try_recv().is_err() {
                break;
            }
        }
    }
    if state.sessions.values().any(Session::active) {
        if state.timer.is_empty() {
            state.timer = cx.start_interval(0.1);
        }
    } else if !state.timer.is_empty() {
        cx.stop_timer(state.timer);
        state.timer = Timer::default();
    }
}
fn started(session: &mut Session) {
    session.state = "starting";
    session.started = Instant::now();
    session.last_frame_at = Instant::now();
    if let Some(reply) = session.start_reply.take() {
        reply.send(Ok(session.snapshot()));
    }
}
fn begin_capture(cx: &mut Cx, session: &mut Session, device: Option<AudioDeviceId>) {
    let Some(device) = device else {
        // Installing the lease owner's silent callback initializes native
        // enumeration. It does not select or start a microphone.
        cx.audio_input(0, |_, _| {});
        session.state = "waiting_device";
        session.started = Instant::now();
        return;
    };
    let shared = session.shared.clone();
    cx.audio_input(0, move |info, buffer| {
        if shared.stop.load(Ordering::Acquire) || shared.cancel.load(Ordering::Acquire) {
            return;
        }
        if !(1..=2).contains(&buffer.channel_count()) || !info.sample_rate.is_finite() {
            shared.invalid.store(true, Ordering::Release);
            return;
        }
        for from in (0..buffer.frame_count()).step_by(CHUNK) {
            let len = (buffer.frame_count() - from).min(CHUNK);
            let mut chunk = Chunk {
                rate: info.sample_rate as u32,
                len,
                samples: [0.0; CHUNK],
            };
            for channel in 0..buffer.channel_count() {
                let source = buffer.channel(channel);
                for n in 0..len {
                    chunk.samples[n] += source[from + n] / buffer.channel_count() as f32;
                }
            }
            if shared.queue.push(chunk).is_err() {
                shared.overflow.store(true, Ordering::Release);
                return;
            }
            shared.frames.fetch_add(len as u64, Ordering::Relaxed);
        }
    });
    session.device = Some(device);
    cx.use_audio_inputs(&[device]);
    started(session);
}
fn begin_playback(cx: &mut Cx, session: &mut Session, device: Option<AudioDeviceId>) {
    let Some(device) = device else {
        cx.audio_output(0, |_, buffer| buffer.data.fill(0.0));
        session.state = "waiting_device";
        session.started = Instant::now();
        return;
    };
    let Some(pcm) = session.pcm.clone() else {
        session.finish(
            cx,
            "failed",
            Some("invalid_audio: Missing decoded audio".into()),
        );
        return;
    };
    let shared = session.shared.clone();
    let mut cursor = 0.0f64;
    cx.audio_output(0, move |info, buffer| {
        buffer.data.fill(0.0);
        if shared.stop.load(Ordering::Acquire) || shared.cancel.load(Ordering::Acquire) {
            return;
        }
        if !info.sample_rate.is_finite() || info.sample_rate < 8000.0 {
            shared.invalid.store(true, Ordering::Release);
            return;
        }
        let step = pcm.rate as f64 / info.sample_rate;
        let frames = pcm.frames();
        for frame in 0..buffer.frame_count() {
            let source = cursor as usize;
            if source >= frames {
                shared.ended.store(true, Ordering::Release);
                break;
            }
            for channel in 0..buffer.channel_count() {
                let c = channel.min(pcm.channels as usize - 1);
                let a = pcm.pcm_interleaved_f32[source * pcm.channels as usize + c];
                let b = pcm.pcm_interleaved_f32
                    [source.saturating_add(1).min(frames - 1) * pcm.channels as usize + c];
                buffer.channel_mut(channel)[frame] =
                    (a + (b - a) * (cursor.fract() as f32)).clamp(-1.0, 1.0);
            }
            cursor += step;
            shared.frames.fetch_add(1, Ordering::Relaxed);
        }
    });
    session.device = Some(device);
    cx.use_audio_outputs(&[device]);
    started(session);
}
