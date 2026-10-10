//! The `wasm` service (ADR 0011, feature `wasm-functions`): an app's own
//! functions, written in Rust and shipped in its bundle as `fns/*.wasm`,
//! run for that app by `octosense-wasm-host` (Wasmtime, compiled by
//! Cranelift, with a deadline and a memory cap). They are for light
//! computing and algorithms beside the app's script: parsing, ranking,
//! scheduling, diffing. Splash's own compute handles kernels.
//!
//! - `wasm.functions` answers what the app's modules export, how they
//!   loaded and how each function has run so far.
//! - `wasm.<function>` calls one. Its input is the request's arguments: a
//!   string as its text, anything else as JSON. Its output comes back as
//!   JSON, or as `{"text": …}` when it is not JSON. Its own error, a trap or
//!   its deadline is the request's error.
//!
//! **Whose code.** A module comes only from the calling app's own admitted
//! bundle (digest-checked, [`script_apps::admitted_bundle`]): no argument
//! names a file, and no app reaches another's functions. The Card runner's
//! gate and the tool executor both require the app's `wasm` capability, and
//! the service checks the admitted manifest again before it loads anything.
//!
//! **Where it runs.** A bounded worker per active app caches compiled Programs,
//! never guest instances. Every invocation starts with fresh memory/globals/tables.
//! Admission, cancellation and deadline are checked before execution and delivery;
//! an update or withdrawal invalidates loaded Programs. Workers exit when idle.
//! The host-only disk cache holds compiled code keyed by module digest, not data.
//!
//! **Components** (ADR 0014). A `fns/*.wasm` may also be a WebAssembly
//! component, built with plain `cargo build --target wasm32-wasip2`. Each of
//! its exported functions is callable as `wasm.<name>` with JSON (snake_case
//! names; `octosense_wasm_host::component` maps the values). Unlike a
//! module's, a component's instance lives on in the worker between calls,
//! so it can keep a document or a cache; a trap, a deadline, a changed grant
//! or the worker's exit (a minute without calls) ends it. Its filesystem is
//! the app's storage, the jail its script's `fs.*` sees, decided per call by
//! the same rules as an engine's ([`crate::host_tools::areas::app_area`]):
//! none without the `storage` capability or a signed-in account, and what
//! is left of the quota is what a call may add (a write past it fails inside
//! the component, as a full disk).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use octosense_appstore::services::{HostService, Replier, ServiceCall, ServiceHost};
use octosense_wasm_host::component::{
    self, ComponentInstance, ComponentProgram, Grants, HostCalls,
};
use octosense_wasm_host::{Limits, Program, Runtime};
use serde_json::{json, Value};

use crate::host_tools::areas::{self, AreaEnv};
use crate::host_tools::script_apps;

const MAX_MODULES: usize = 8;
const MAX_WORKERS: usize = 4;
const MAX_QUEUED_PER_APP: usize = 4;
const MAX_INPUT_BYTES: usize = 8 << 20;
const MAX_BUFFERED_BYTES: usize = 32 << 20;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const WORKER_IDLE: Duration = Duration::from_secs(5);
/// How long a worker that holds a component's live instance waits for the
/// next call before it exits (and the instance with it).
const COMPONENT_IDLE: Duration = Duration::from_secs(60);

/// Where an app's component works: the shell's own rules, or a test's.
static AREA_ENV: Mutex<Option<Arc<dyn AreaEnv>>> = Mutex::new(None);

fn area_env() -> Arc<dyn AreaEnv> {
    AREA_ENV
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(areas::shell)
        .clone()
}

static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();

// ------------------------------------------------- a component's host calls

/// A component's call to one of its app's host services (`octosense:host`),
/// waiting for the UI thread ([`pump_host_calls`]), where its app's script
/// would make it.
struct HostCall {
    call: ServiceCall,
    deadline: Instant,
    done: SyncSender<Result<String, String>>,
}

static HOST_CALLS: Mutex<Vec<HostCall>> = Mutex::new(Vec::new());
/// Dispatched calls by their request key, until answered or past their
/// deadline.
type Waiting = HashMap<usize, (SyncSender<Result<String, String>>, Instant)>;
static HOST_WAITING: Mutex<Option<Waiting>> = Mutex::new(None);
/// Component calls are dispatched under keys of their own, apart from any
/// isolate's.
static NEXT_HOST_KEY: AtomicUsize = AtomicUsize::new(1 << 50);

/// An app's host services as its components reach them: the families its
/// admitted manifest grants (a system app's own namespace too), dispatched
/// on the UI thread as its script's `host.request` would be, but never with
/// a sheet or a prompt, so only the methods a background surface may call.
struct AppHostCalls {
    app: String,
    host_dir: PathBuf,
    families: BTreeSet<String>,
}

impl HostCalls for AppHostCalls {
    fn request(&self, service: &str, args: &str, deadline: Instant) -> Result<String, String> {
        let family = service.split('.').next().unwrap_or("");
        // The app's worker is busy running this very call.
        if family == "wasm" {
            return Err(
                "a component cannot call wasm.*: its app's functions are already running it".into(),
            );
        }
        let own = self
            .app
            .strip_prefix(octosense_appstore::system::SYSTEM_ID_PREFIX)
            == Some(family);
        if !self.families.contains(family) && !own {
            return Err(format!(
                "{} was not granted the {family} service, which {service} needs",
                self.app
            ));
        }
        let args: Value = serde_json::from_str(args)
            .map_err(|e| format!("{service}: the arguments are not JSON: {e}"))?;
        let (done, answer) = sync_channel(1);
        HOST_CALLS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(HostCall {
                call: ServiceCall {
                    app_id: self.app.clone(),
                    service: service.to_string(),
                    args,
                    from_sheet: false,
                    may_prompt: false,
                    host_dir: self.host_dir.clone(),
                },
                deadline,
                done,
            });
        makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
        answer
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|_| {
                Err(format!(
                    "{service} did not answer before the call's deadline"
                ))
            })
    }
}

/// Dispatch the components' queued host calls and deliver their answers;
/// on the UI thread ([`crate::host_tools::pump`]).
pub fn pump_host_calls() {
    struct NoSheets;
    impl ServiceHost for NoSheets {
        fn open_sheet(&mut self, _body: String) {}
        fn close_sheet(&mut self) {}
    }
    let queued = std::mem::take(&mut *HOST_CALLS.lock().unwrap_or_else(|e| e.into_inner()));
    let mut waiting = HOST_WAITING.lock().unwrap_or_else(|e| e.into_inner());
    let waiting = waiting.get_or_insert_with(HashMap::new);
    for HostCall {
        call,
        deadline,
        done,
    } in queued
    {
        if Instant::now() >= deadline {
            continue;
        }
        let key = NEXT_HOST_KEY.fetch_add(1, Ordering::Relaxed);
        waiting.insert(key, (done, deadline));
        octosense_appstore::services::dispatch(call, key, 1, &mut NoSheets);
    }
    if waiting.is_empty() {
        return;
    }
    let keys: Vec<usize> = waiting.keys().copied().collect();
    for (key, _, result) in octosense_appstore::services::take_replies_for(&keys) {
        if let Some((done, _)) = waiting.remove(&key) {
            let _ = done.try_send(result);
        }
    }
    // A call whose component stopped waiting goes nowhere.
    let now = Instant::now();
    let expired: Vec<usize> = waiting
        .iter()
        .filter(|(_, (_, deadline))| *deadline <= now)
        .map(|(key, _)| *key)
        .collect();
    for key in expired {
        waiting.remove(&key);
        octosense_appstore::services::cancel_heap(key);
    }
}
static WORKERS: Mutex<Option<HashMap<String, Worker>>> = Mutex::new(None);
static BUFFERED_BYTES: AtomicUsize = AtomicUsize::new(0);

struct Worker {
    jobs: SyncSender<Job>,
    retired: Arc<AtomicBool>,
    admission: Arc<AdmissionEpoch>,
}

/// Enqueue only clones this host-owned identity. Bundle IO, signature checks
/// and digesting run on the worker, including the first cold admission.
#[derive(Default)]
struct AdmissionEpoch {
    admitted: OnceLock<Result<Admission, String>>,
    obsolete: AtomicBool,
}

/// Removed only after the worker has dropped its queue and compiled modules.
/// A closing worker keeps its slot until then, even during revision churn.
struct RetireWorker {
    app: String,
    retired: Arc<AtomicBool>,
}
impl Drop for RetireWorker {
    fn drop(&mut self) {
        let mut workers = WORKERS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(workers) = workers.as_mut() {
            if workers
                .get(&self.app)
                .is_some_and(|worker| Arc::ptr_eq(&worker.retired, &self.retired))
            {
                workers.remove(&self.app);
            }
        }
    }
}

/// Includes executing input: cancelling a queued request cannot free its
/// reservation until the worker actually drops those bytes.
struct Reservation(usize);
impl Reservation {
    fn acquire(bytes: usize) -> Result<Self, String> {
        BUFFERED_BYTES
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes)
                    .filter(|total| *total <= MAX_BUFFERED_BYTES)
            })
            .map(|_| Self(bytes))
            .map_err(|_| "wasm input queue is full; try again later".into())
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        BUFFERED_BYTES.fetch_sub(self.0, Ordering::AcqRel);
    }
}

/// Authority is the signed, digest-checked bundle, never request arguments.
/// Comparing the whole admitted manifest includes the version and grants.
#[derive(Clone, Debug, PartialEq)]
struct Admission {
    root: PathBuf,
    bundle: PathBuf,
    manifest: Value,
}
impl Admission {
    fn current(app: &str) -> Result<Self, String> {
        let (root, bundle) = script_apps::admitted_bundle(app)?;
        let loaded = script_apps::from_bundle(&bundle)?;
        if !loaded.families.contains("wasm") {
            return Err(format!("{app} was not granted the wasm service"));
        }
        Ok(Self {
            root,
            bundle,
            manifest: loaded.manifest,
        })
    }
    fn verify(&self, app: &str) -> Result<(), String> {
        if Self::current(app)? != *self {
            return Err("wasm app admission changed; retry from the current app".into());
        }
        Ok(())
    }
}

struct Job {
    method: String,
    input: Vec<u8>,
    /// The request's arguments were a JSON string, and `input` is its text.
    text: bool,
    host_dir: PathBuf,
    admission: Arc<AdmissionEpoch>,
    deadline: Instant,
    reply: Replier,
    _reservation: Reservation,
}
impl Job {
    fn live(&self) -> bool {
        self.reply.is_pending() && Instant::now() < self.deadline
    }
    fn check(&self, app: &str) -> Result<&Admission, String> {
        if !self.live() {
            return Err("wasm invocation cancelled or past its deadline".into());
        }
        if self.admission.obsolete.load(Ordering::Acquire) {
            return Err("wasm app admission changed; retry from the current app".into());
        }
        let admitted = self
            .admission
            .admitted
            .get_or_init(|| Admission::current(app));
        let verified = admitted
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|admission| {
                admission.verify(app)?;
                Ok(admission)
            });
        let admission = match verified {
            Ok(admission) => admission,
            Err(error) => {
                self.admission.obsolete.store(true, Ordering::Release);
                return Err(error);
            }
        };
        // The cache is host-owned, not a caller-selected or app directory.
        if self.host_dir != admission.root.join(".host") {
            return Err("wasm host directory does not belong to this app store".into());
        }
        if !self.live() {
            return Err("wasm invocation cancelled or past its deadline".into());
        }
        Ok(admission)
    }
}

/// Serialize into bounded storage, rather than allocating an unlimited JSON
/// buffer and only then discovering the request is too large.
fn input_bytes(args: &Value) -> Result<Vec<u8>, String> {
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_INPUT_BYTES.saturating_sub(self.0.len()) {
                return Err(std::io::Error::other(format!(
                    "wasm input exceeds {} MiB",
                    MAX_INPUT_BYTES >> 20
                )));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    if let Value::String(text) = args {
        if text.len() > MAX_INPUT_BYTES {
            return Err(format!("wasm input exceeds {} MiB", MAX_INPUT_BYTES >> 20));
        }
        return Ok(text.as_bytes().to_vec());
    }
    let mut out = Bounded(Vec::new());
    serde_json::to_writer(&mut out, args).map_err(|error| error.to_string())?;
    Ok(out.0)
}

pub struct WasmService;
impl HostService for WasmService {
    fn family(&self) -> &'static str {
        "wasm"
    }
    fn timeout(&self, _call: &ServiceCall) -> Duration {
        REQUEST_TIMEOUT
    }
    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if call.from_sheet {
            reply.send(Err("the wasm service has no sheet".into()));
            return;
        }
        if !reply.is_pending() {
            return;
        }
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let prepared = (|| {
            let input = input_bytes(&call.args)?;
            let reservation = Reservation::acquire(input.len())?;
            Ok((input, reservation))
        })();
        let (input, reservation) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                reply.send(Err(error));
                return;
            }
        };
        let app = call.app_id.clone();
        let mut job = Job {
            method: call.method().to_string(),
            text: matches!(call.args, Value::String(_)),
            host_dir: call.host_dir,
            input,
            admission: Arc::default(),
            deadline,
            reply,
            _reservation: reservation,
        };
        if !job.live() {
            return;
        }
        let mut registry = WORKERS.lock().unwrap_or_else(|e| e.into_inner());
        let workers = registry.get_or_insert_with(HashMap::new);
        if let Some(worker) = workers.get_mut(&app) {
            if worker.retired.load(Ordering::Acquire) {
                job.reply
                    .send(Err("wasm worker is closing; try again later".into()));
                return;
            }
            if worker.admission.obsolete.load(Ordering::Acquire) {
                worker.admission = Arc::default();
            }
            job.admission = worker.admission.clone();
            match worker.jobs.try_send(job) {
                Ok(()) => return,
                Err(TrySendError::Full(job)) => {
                    job.reply
                        .send(Err("wasm app queue is full; try again later".into()));
                    return;
                }
                Err(TrySendError::Disconnected(job)) => {
                    // The worker's guard removes its slot after all resources
                    // have dropped. Do not create an overlapping replacement.
                    job.reply
                        .send(Err("wasm worker is closing; try again later".into()));
                    return;
                }
            };
        }
        if workers.len() >= MAX_WORKERS {
            job.reply
                .send(Err("wasm workers are busy; try again later".into()));
            return;
        }
        let (jobs, queue) = sync_channel(MAX_QUEUED_PER_APP);
        let retired = Arc::new(AtomicBool::new(false));
        let admission = job.admission.clone();
        // Reserve the worker slot and queue before spawning, then release the
        // registry mutex: neither thread creation nor admission holds it.
        if jobs.try_send(job).is_err() {
            unreachable!("new wasm queue has room");
        }
        workers.insert(
            app.clone(),
            Worker {
                jobs,
                retired: retired.clone(),
                admission,
            },
        );
        // Keep ownership outside the spawn closure too, so a spawn failure
        // can explicitly answer every request accepted during this interval.
        let queue = Arc::new(Mutex::new(Some(queue)));
        drop(registry);
        self.start_worker(app, retired, queue);
    }
}

impl WasmService {
    fn start_worker(
        &self,
        app: String,
        retired: Arc<AtomicBool>,
        queue: Arc<Mutex<Option<Receiver<Job>>>>,
    ) {
        let name = app.clone();
        let worker_queue = queue.clone();
        let worker_retired = retired.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("wasm {app}"))
            .spawn(move || {
                let _retire = RetireWorker {
                    app: name.clone(),
                    retired: worker_retired.clone(),
                };
                let queue = worker_queue
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .unwrap();
                work(name, queue, &worker_retired);
            });
        if let Err(error) = spawned {
            let registry = WORKERS.lock().unwrap_or_else(|e| e.into_inner());
            retired.store(true, Ordering::Release);
            drop(registry);
            if let Some(queue) = queue.lock().unwrap_or_else(|e| e.into_inner()).take() {
                for job in queue.try_iter() {
                    job.reply
                        .send(Err(format!("cannot start {app}'s functions: {error}")));
                }
            }
            drop(RetireWorker { app, retired });
        }
    }
}

pub fn register() {
    octosense_appstore::services::register_host_service(Box::new(WasmService));
}

fn work(app: String, jobs: Receiver<Job>, retired: &Arc<AtomicBool>) {
    let mut lab: Option<Lab> = None;
    loop {
        // A component's live instance keeps its state: give the app longer
        // to come back before it is dropped.
        let idle = if lab.as_ref().is_some_and(Lab::holds_instances) {
            COMPONENT_IDLE
        } else {
            WORKER_IDLE
        };
        let job = match jobs.recv_timeout(idle) {
            Ok(job) => job,
            Err(_) => {
                // Serialize idle retirement with enqueue. A sender that won
                // the timeout race still gets its accepted request processed.
                let _workers = WORKERS.lock().unwrap_or_else(|e| e.into_inner());
                if let Ok(job) = jobs.try_recv() {
                    job
                } else {
                    retired.store(true, Ordering::Release);
                    // The outer retirement guard removes the slot only once
                    // this function has dropped the Lab and queued inputs.
                    return;
                }
            }
        };
        let result = process(&app, &mut lab, &job);
        // Cancellation can win while admission is being checked. Replier's
        // pending-request check remains the final isolate boundary.
        if job.live() {
            job.reply.send(result);
        }
    }
}

fn process(app: &str, lab: &mut Option<Lab>, job: &Job) -> Result<Value, String> {
    process_checked(app, lab, job, || {})
}

// The completion boundary is explicit so regressions can replace the real
// admitted bundle precisely after guest execution, without timing sleeps.
fn process_checked(
    app: &str,
    lab: &mut Option<Lab>,
    job: &Job,
    before_delivery: impl FnOnce(),
) -> Result<Value, String> {
    let admission = match job.check(app) {
        Ok(admission) => admission,
        Err(error) => {
            *lab = None;
            return Err(error);
        }
    };
    if lab
        .as_ref()
        .is_some_and(|loaded| loaded.admission != *admission)
    {
        *lab = None;
    }
    if lab.is_none() {
        *lab = Some(Lab::load(app, job)?);
    }
    // Compilation and start-up work must not revive an expired request.
    if let Err(error) = job.check(app) {
        *lab = None;
        return Err(error);
    }
    let result = lab.as_mut().unwrap().answer(
        &job.method,
        &job.input,
        job.text,
        job.deadline,
        job.reply.clone(),
    );
    before_delivery();
    // A withdrawal, changed grant, update or tamper while code runs vetoes
    // its result and releases the old compiled-module references.
    if let Err(error) = job.check(app) {
        *lab = None;
        return Err(error);
    }
    result
}

fn runtime(host_dir: &Path) -> Result<&'static Runtime, String> {
    RUNTIME
        .get_or_init(|| Runtime::new(Limits::default(), Some(host_dir.join("wasm-cache"))))
        .as_ref()
        .map_err(Clone::clone)
}

/// One app's loaded modules.
struct Lab {
    app: String,
    runtime: &'static Runtime,
    modules: Vec<Module>,
    /// Which module exports each function.
    owner: BTreeMap<String, usize>,
    stats: BTreeMap<String, Stats>,
    admission: Admission,
    /// Its host services, as its components reach them (`octosense:host`).
    host_calls: Arc<dyn HostCalls>,
}

/// Compile an installed or updated app's functions into the disk cache in
/// the background, so that its first call does not wait for Cranelift (ADR
/// 0014 phase 3; a phone takes about 0.4 s for a 433 KiB module). From its
/// admitted bundle only, and only with the `wasm` grant; one app at a time,
/// on a thread of its own, never in a worker's place. A call that comes
/// first compiles the same code itself: the cache takes either.
pub fn warm(app: &str) {
    static QUEUE: Mutex<(Vec<String>, bool)> = Mutex::new((Vec::new(), false));
    {
        let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        if !queue.0.iter().any(|queued| queued == app) {
            queue.0.push(app.to_string());
        }
        if std::mem::replace(&mut queue.1, true) {
            return;
        }
    }
    let spawned = std::thread::Builder::new()
        .name("wasm-warm".into())
        .spawn(|| loop {
            let next = {
                let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                if queue.0.is_empty() {
                    queue.1 = false;
                    return;
                }
                queue.0.remove(0)
            };
            match warm_now(&next) {
                Ok((0, _, _)) => {}
                Ok((compiled, files, ms)) => makepad_widgets::log!(
                    "wasm {next}: compiled {compiled} of {files} function files ahead of its first call in {ms:.0} ms"
                ),
                Err(error) => makepad_widgets::log!("wasm {next}: not compiled ahead: {error}"),
            }
        });
    if spawned.is_err() {
        QUEUE.lock().unwrap_or_else(|e| e.into_inner()).1 = false;
    }
}

/// [`warm`]'s work for one app: how many files it compiled, of how many, and
/// how long it took.
fn warm_now(app: &str) -> Result<(usize, usize, f64), String> {
    let admission = Admission::current(app)?;
    let runtime = runtime(&admission.root.join(".host"))?;
    let started = Instant::now();
    let mut files: Vec<PathBuf> = std::fs::read_dir(admission.bundle.join("fns"))
        .map_err(|_| format!("{app}'s bundle has no fns directory"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "wasm")
                && std::fs::symlink_metadata(path)
                    .is_ok_and(|m| m.is_file() && m.len() <= runtime.limits().module_bytes as u64)
        })
        .collect();
    files.sort();
    files.truncate(MAX_MODULES);
    let mut compiled = 0;
    for path in &files {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if runtime.precompile(&bytes).map_err(|e| e.to_string())? {
            compiled += 1;
        }
    }
    Ok((compiled, files.len(), started.elapsed().as_secs_f64() * 1e3))
}

struct Module {
    file: String,
    bytes: usize,
    load_ms: f64,
    from_cache: bool,
    code: Code,
    /// Diagnostic high-water mark only: guest memory is never retained.
    memory_bytes: usize,
    invocations: u64,
}

enum Code {
    /// A core module (ADR 0011): a fresh instance for every call.
    Module(Program),
    /// A component (ADR 0014): one instance, kept between calls, and the
    /// grants it was made with.
    Component {
        program: ComponentProgram,
        live: Option<(ComponentInstance, Grants)>,
        instances: u64,
    },
}

/// How one function has run: in the worker, from input to output.
#[derive(Default)]
struct Stats {
    calls: u64,
    errors: u64,
    total_us: f64,
    max_us: f64,
}

impl Lab {
    fn load(app: &str, job: &Job) -> Result<Lab, String> {
        let admission = job.check(app)?.clone();
        let runtime = runtime(&job.host_dir)?;
        Lab::from_bundle(app, runtime, admission, || job.check(app).map(drop))
    }

    /// The admitted bundle's `fns/*.wasm`, loaded; `check` ends it early (a
    /// cancelled request, a changed admission).
    fn from_bundle(
        app: &str,
        runtime: &'static Runtime,
        admission: Admission,
        check: impl Fn() -> Result<(), String>,
    ) -> Result<Lab, String> {
        let bundle = &admission.bundle;
        let mut files: Vec<PathBuf> = std::fs::read_dir(bundle.join("fns"))
            .map_err(|_| format!("{app}'s bundle has no fns directory"))?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().is_some_and(|ext| ext == "wasm")
                    && std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
            })
            .collect();
        files.sort();
        if files.is_empty() || files.len() > MAX_MODULES {
            return Err(format!(
                "{app}'s bundle must carry 1 to {MAX_MODULES} fns/*.wasm modules"
            ));
        }
        let mut lab = Lab {
            app: app.to_string(),
            runtime,
            modules: Vec::new(),
            owner: BTreeMap::new(),
            stats: BTreeMap::new(),
            host_calls: Arc::new(AppHostCalls {
                app: app.to_string(),
                host_dir: admission.root.join(".host"),
                families: admission.manifest["capabilities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c.as_str().map(str::to_string))
                    .collect(),
            }),
            admission: admission.clone(),
        };
        for path in files {
            check()?;
            let file = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if std::fs::metadata(&path).map_err(|e| e.to_string())?.len()
                > runtime.limits().module_bytes as u64
            {
                return Err(format!("{file}: module exceeds the size limit"));
            }
            let bytes = std::fs::read(&path).map_err(|e| format!("{file}: {e}"))?;
            let started = Instant::now();
            let (code, functions, from_cache) = if component::is_component(&bytes) {
                let program = runtime
                    .load_component(&bytes)
                    .map_err(|e| format!("{file}: {e}"))?;
                let names: Vec<String> = program.exports().iter().map(|e| e.name.clone()).collect();
                let from_cache = program.from_cache();
                (
                    Code::Component {
                        program,
                        live: None,
                        instances: 0,
                    },
                    names,
                    from_cache,
                )
            } else {
                let program = runtime.load(&bytes).map_err(|e| format!("{file}: {e}"))?;
                let names = program.functions().to_vec();
                let from_cache = program.from_cache();
                (Code::Module(program), names, from_cache)
            };
            let load_ms = started.elapsed().as_secs_f64() * 1e3;
            for function in &functions {
                if lab
                    .owner
                    .insert(function.clone(), lab.modules.len())
                    .is_some()
                {
                    return Err(format!(
                        "{file}: {function} is exported by another module too"
                    ));
                }
            }
            makepad_widgets::log!(
                "wasm {app}: {file} ({} KiB{}) {} in {load_ms:.1} ms: {}",
                bytes.len() / 1024,
                if matches!(code, Code::Component { .. }) {
                    ", a component"
                } else {
                    ""
                },
                if from_cache {
                    "loaded from the cache"
                } else {
                    "compiled"
                },
                functions.join(", ")
            );
            lab.modules.push(Module {
                file,
                bytes: bytes.len(),
                load_ms,
                from_cache,
                code,
                memory_bytes: 0,
                invocations: 0,
            });
        }
        Ok(lab)
    }

    /// Whether a component's instance is alive (its state worth keeping).
    fn holds_instances(&self) -> bool {
        self.modules
            .iter()
            .any(|m| matches!(&m.code, Code::Component { live: Some(_), .. }))
    }

    fn answer(
        &mut self,
        method: &str,
        input: &[u8],
        text: bool,
        deadline: Instant,
        reply: Replier,
    ) -> Result<Value, String> {
        if method == "functions" {
            // Also in the log: where there is no remote instrument (an
            // Android build), the numbers are read from logcat.
            let described = self.describe();
            makepad_widgets::log!("wasm {}: {described}", self.app);
            return Ok(described);
        }
        let name = if self.owner.contains_key(method) {
            method.to_string()
        } else {
            component::snake(method)
        };
        let Some(&index) = self.owner.get(&name) else {
            return Err(format!("{} has no function {method:?}", self.app));
        };
        let module = &mut self.modules[index];
        let program = match &mut module.code {
            Code::Module(program) => program,
            Code::Component { .. } => {
                return self.answer_component(index, &name, input, text, deadline, reply)
            }
        };
        let started = Instant::now();
        let mut instance = self
            .runtime
            .instantiate_guarded(program, deadline, move || reply.is_pending())
            .map_err(|error| error.to_string())?;
        module.invocations += 1;
        let result = instance.call(method, input);
        let us = started.elapsed().as_secs_f64() * 1e6;
        module.memory_bytes = module.memory_bytes.max(instance.memory_bytes());
        for line in instance.take_logs() {
            makepad_widgets::log!("wasm {}: {line}", self.app);
        }
        // Drop guest memory, mutable globals, tables and logs after every
        // invocation, including successful calls and guest-returned errors.
        drop(instance);
        let stats = self.stats.entry(method.to_string()).or_default();
        stats.calls += 1;
        stats.total_us += us;
        stats.max_us = stats.max_us.max(us);
        let output = result.map_err(|error| {
            stats.errors += 1;
            error.to_string()
        })?;
        Ok(serde_json::from_slice(&output)
            .unwrap_or_else(|_| json!({"text": String::from_utf8_lossy(&output)})))
    }

    /// One call to a component's function, on its live instance (made, or
    /// remade, when there is none, it was spent, or its grants changed).
    fn answer_component(
        &mut self,
        index: usize,
        name: &str,
        input: &[u8],
        text: bool,
        deadline: Instant,
        reply: Replier,
    ) -> Result<Value, String> {
        let args: Value = if text {
            Value::String(String::from_utf8_lossy(input).into_owned())
        } else if input.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(input).map_err(|e| format!("the arguments are not JSON: {e}"))?
        };
        // Decided for every call: a sign-out or a revoked grant applies to
        // the next call, and the quota is what is left of it now.
        let area = areas::app_area(&*area_env(), &self.app, true).ok();
        let grants = Grants {
            storage_dir: area.as_ref().map(|a| a.root.clone()),
            read_only: false,
        };
        let app = self.app.clone();
        let host_calls = self.host_calls.clone();
        let module = &mut self.modules[index];
        let Code::Component {
            program,
            live,
            instances,
        } = &mut module.code
        else {
            unreachable!("answer_component is called for components");
        };
        let started = Instant::now();
        if live
            .as_ref()
            .is_some_and(|(instance, made)| instance.spent() || *made != grants)
        {
            *live = None;
        }
        if live.is_none() {
            let pending = reply.clone();
            let instance = self
                .runtime
                .instantiate_component(
                    program,
                    &grants,
                    Some((deadline, Box::new(move || pending.is_pending()))),
                )
                .map_err(|error| error.to_string())?;
            let mut instance = instance;
            instance.set_host_calls(Some(host_calls));
            *instances += 1;
            *live = Some((instance, grants.clone()));
        }
        let (instance, _) = live.as_mut().expect("made above");
        module.invocations += 1;
        // A write past the quota fails inside the component, as a full disk.
        instance.set_storage_budget(area.and_then(|a| a.quota_left));
        let result = instance.call_json_guarded(name, &args, deadline, move || reply.is_pending());
        let us = started.elapsed().as_secs_f64() * 1e6;
        for line in instance.take_logs() {
            makepad_widgets::log!("wasm {app}: {line}");
        }
        if instance.spent() {
            *live = None;
        }
        let stats = self.stats.entry(name.to_string()).or_default();
        stats.calls += 1;
        stats.total_us += us;
        stats.max_us = stats.max_us.max(us);
        result.map_err(|error| {
            stats.errors += 1;
            error.to_string()
        })
    }

    fn describe(&self) -> Value {
        let modules: Vec<Value> = self
            .modules
            .iter()
            .map(|m| match &m.code {
                Code::Module(_) => json!({"file": m.file, "kind": "module", "bytes": m.bytes, "load_ms": round(m.load_ms),
                    "from_cache": m.from_cache, "memory_bytes": m.memory_bytes, "invocations": m.invocations, "renewed": m.invocations.saturating_sub(1), "instance_policy": "fresh-per-call"}),
                Code::Component { program, live, instances } => json!({"file": m.file, "kind": "component", "bytes": m.bytes, "load_ms": round(m.load_ms),
                    "from_cache": m.from_cache, "invocations": m.invocations, "instances": instances, "instance_policy": "kept-between-calls",
                    "live": live.is_some(), "storage": live.as_ref().map(|(_, g)| if g.storage_dir.is_some() { "app folder" } else { "none" }),
                    "storage_left": live.as_ref().and_then(|(i, _)| i.storage_budget()),
                    "network": program.uses_network(),
                    "exports": program.exports().iter().map(|e| json!({"name": e.name, "wit": e.wit_name,
                        "params": e.params.iter().map(|(n, t)| json!([n, t])).collect::<Vec<_>>(), "result": e.result})).collect::<Vec<_>>(),
                    "skipped": program.skipped().iter().map(|(n, why)| json!({"name": n, "why": why})).collect::<Vec<_>>()}),
            })
            .collect();
        let stats: serde_json::Map<String, Value> = self
            .stats
            .iter()
            .map(|(name, s)| {
                let mean = if s.calls == 0 { 0.0 } else { s.total_us / s.calls as f64 };
                (name.clone(), json!({"calls": s.calls, "errors": s.errors, "mean_us": round(mean), "max_us": round(s.max_us)}))
            })
            .collect();
        json!({"functions": self.owner.keys().collect::<Vec<_>>(), "modules": modules, "stats": stats})
    }
}

fn round(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_host::app_peers::host_tools::{HostToolCall, ToolExecutor, ToolReply};
    use crate::host_tools::script_apps::{self, HostServiceExecutor};
    use std::sync::Arc;
    use std::time::Duration;

    /// Wasm Lab's agent gets exactly its three function tools, each routed
    /// to the app's own function on the `wasm` service.
    #[test]
    fn wasm_lab_offers_its_functions_as_tools() {
        let dir = script_apps::tests::stamped_bundle("wasmlab", "tools", |_, _| {});
        let loaded = script_apps::from_bundle(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let names: Vec<&str> = loaded
            .tools
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert_eq!(
            names,
            ["wasmlab.find_slots", "wasmlab.rank", "wasmlab.diff"]
        );
        assert_eq!(loaded.host_methods["wasmlab.find_slots"], "wasm.find_slots");
        assert_eq!(loaded.host_methods["wasmlab.rank"], "wasm.fuzzy_rank");
        assert_eq!(loaded.host_methods["wasmlab.diff"], "wasm.text_diff");
        assert!(loaded
            .tools
            .iter()
            .all(|t| t["risk"] == "read" && t["output_schema"]["type"] == "object"));
        assert_eq!(loaded.families.iter().collect::<Vec<_>>(), ["wasm"]);
        assert_eq!(loaded.generic, ["ask_user_question"]);
    }

    struct NoSheet;
    impl ServiceHost for NoSheet {
        fn open_sheet(&mut self, _body: String) {}
        fn close_sheet(&mut self) {}
    }

    static NEXT_HEAP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1 << 40);

    /// What the app's script would get from `host.request(service, args)`.
    fn request(app: &str, service: &str, args: Value, host_dir: &Path) -> Result<Value, String> {
        let heap = NEXT_HEAP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let call = ServiceCall {
            app_id: app.into(),
            service: service.into(),
            args,
            from_sheet: false,
            may_prompt: true,
            host_dir: host_dir.into(),
        };
        octosense_appstore::services::dispatch(call, heap, 1, &mut NoSheet);
        for _ in 0..1000 {
            if let Some((_, _, answer)) =
                octosense_appstore::services::take_replies_for(&[heap]).pop()
            {
                return answer.map(|text| serde_json::from_str(&text).unwrap());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("{service} was never answered");
    }

    /// Register a stamped copy of `apps/wasmlab/bundle` as the system app
    /// `id`, as a build that selects it does.
    fn ship(id: &'static str, tag: &str, edit: impl FnOnce(&Path, &mut Value)) {
        let dir = script_apps::tests::stamped_bundle("wasmlab", tag, |dir, manifest| {
            manifest["id"] = json!(id);
            edit(dir, manifest);
        });
        let packed = octosense_app_hub::pack::pack_system_app(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        octosense_appstore::system::register_system_app(octosense_appstore::system::SystemApp {
            id,
            name: "Wasm Lab",
            pack: Box::leak(packed.pack_json.into_boxed_str()),
            assets: &[],
        });
    }

    // A real guest deliberately retains the previous input in both linear
    // memory and a mutable global. Reusing the old instance leaks it on recall.
    const STATE_GUEST: &str = r#"(module
      (memory (export "memory") 1)
      (global $size (mut i32) (i32.const 0))
      (data (i32.const 8) "\01failed")
      (func (export "octo_alloc") (param i32) (result i32) (i32.const 4096))
      (func (export "octo_free") (param i32 i32))
      (func $remember (param $p i32) (param $n i32)
        (global.set $size (local.get $n))
        (memory.copy (i32.const 65) (local.get $p) (local.get $n)))
      (func (export "remember") (param i32 i32) (result i64)
        (call $remember (local.get 0) (local.get 1)) (i64.const 1))
      (func (export "remember_fail") (param i32 i32) (result i64)
        (call $remember (local.get 0) (local.get 1)) (i64.const 34359738375))
      (func (export "recall") (param i32 i32) (result i64)
        (i64.or (i64.const 274877906944) (i64.extend_i32_u (i32.add (global.get $size) (i32.const 1))))))"#;

    fn ship_state(id: &'static str, version: &str, granted: bool) {
        ship(id, "state", |dir, manifest| {
            manifest["version"] = json!(version);
            manifest["capabilities"] = if granted { json!(["wasm"]) } else { json!([]) };
            manifest.as_object_mut().unwrap().remove("agent");
            std::fs::remove_file(dir.join("tools.json")).unwrap();
            std::fs::remove_file(dir.join("AGENT.md")).unwrap();
            std::fs::remove_file(dir.join("fns/wasmlab.wasm")).unwrap();
            std::fs::write(
                dir.join("fns/state.wasm"),
                wat::parse_str(if version == "2.0.0" {
                    STATE_GUEST.replace(
                        "(global $size (mut i32) (i32.const 0))",
                        "(global $size (mut i32) (i32.const 3)) (data (i32.const 65) \"new\")",
                    )
                } else {
                    STATE_GUEST.to_string()
                })
                .unwrap(),
            )
            .unwrap();
        });
    }

    /// A pending reply to `app`'s request, as dispatch hands a service one.
    fn pending_reply(app: &str, host_dir: &Path) -> (usize, Replier) {
        struct Capture(Arc<Mutex<Option<Replier>>>);
        impl HostService for Capture {
            fn family(&self) -> &'static str {
                "wasm_capture"
            }
            fn call(&mut self, _: ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
                *self.0.lock().unwrap() = Some(reply);
            }
        }
        let captured = Arc::new(Mutex::new(None));
        octosense_appstore::services::register_host_service(Box::new(Capture(captured.clone())));
        let heap = NEXT_HEAP.fetch_add(1, Ordering::Relaxed);
        octosense_appstore::services::dispatch(
            ServiceCall {
                app_id: app.into(),
                service: "wasm_capture.capture".into(),
                args: json!({}),
                from_sheet: false,
                may_prompt: false,
                host_dir: host_dir.into(),
            },
            heap,
            1,
            &mut NoSheet,
        );
        let reply = captured.lock().unwrap().take().unwrap();
        (heap, reply)
    }

    fn captured_job(app: &str, method: &str, host_dir: &Path) -> (usize, Job) {
        let (heap, reply) = pending_reply(app, host_dir);
        (
            heap,
            Job {
                method: method.into(),
                input: Vec::new(),
                text: false,
                host_dir: host_dir.into(),
                admission: Arc::new(AdmissionEpoch {
                    admitted: OnceLock::from(Ok(Admission::current(app).unwrap())),
                    obsolete: AtomicBool::new(false),
                }),
                deadline: Instant::now() + REQUEST_TIMEOUT,
                reply,
                _reservation: Reservation::acquire(0).unwrap(),
            },
        )
    }

    #[test]
    fn bounded_inputs_reject_strings_and_json_before_queueing() {
        assert!(input_bytes(&json!("x".repeat(MAX_INPUT_BYTES + 1))).is_err());
        // String escaping expands an otherwise smaller tree beyond the limit.
        assert!(input_bytes(&json!({"x": "\n".repeat(MAX_INPUT_BYTES / 2)})).is_err());
        assert_eq!(input_bytes(&json!("plain")).unwrap(), b"plain");
        assert_eq!(input_bytes(&json!({"n": 1})).unwrap(), br#"{"n":1}"#);
    }

    #[test]
    fn invocation_isolation_revocation_cancellation_and_queue_bounds() {
        const CHILD: &str = "OCTOSENSE_TEST_WASM_ISOLATION";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "wasm_service::tests::invocation_isolation_revocation_cancellation_and_queue_bounds", "--nocapture"])
                .env(CHILD, "1")
                .env("OCTOSENSE_HUB_CATALOG", "legacy")
                .output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let root = std::env::temp_dir().join(format!("wasm-isolation-{}", uuid::Uuid::new_v4()));
        let host_dir = root.join(".host");
        // This exact test runs alone in its child process. Keep the env
        // override and registered root identical even under an isolated suite.
        std::env::set_var("OCTOSENSE_APP_DATA", &root);
        octosense_appstore::set_data_root(root.clone());
        ship_state("os.wasmstate", "1.0.0", true);
        register();
        // Real dispatch into a held cold queue must neither unpack nor verify
        // a bundle on this (UI) thread. The old synchronous admission path
        // creates .system here and populates the identity before returning.
        let (cold_sender, cold_queue) = sync_channel(MAX_QUEUED_PER_APP);
        let cold_epoch = Arc::new(AdmissionEpoch::default());
        WORKERS
            .lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .insert(
                "os.wasmstate".into(),
                Worker {
                    jobs: cold_sender,
                    retired: Arc::new(AtomicBool::new(false)),
                    admission: cold_epoch.clone(),
                },
            );
        let cold_heap = NEXT_HEAP.fetch_add(1, Ordering::Relaxed);
        octosense_appstore::services::dispatch(
            ServiceCall {
                app_id: "os.wasmstate".into(),
                service: "wasm.recall".into(),
                args: json!(null),
                from_sheet: false,
                may_prompt: false,
                host_dir: host_dir.clone(),
            },
            cold_heap,
            1,
            &mut NoSheet,
        );
        assert!(cold_epoch.admitted.get().is_none());
        assert!(
            !root.join(".system").exists(),
            "enqueue must not touch bundle storage"
        );
        let cold_job = cold_queue.try_recv().unwrap();
        assert!(process("os.wasmstate", &mut None, &cold_job).is_ok());
        assert!(cold_epoch.admitted.get().unwrap().is_ok());
        assert!(
            root.join(".system").exists(),
            "the worker performed actual admission"
        );
        octosense_appstore::services::cancel_heap(cold_heap);
        drop(cold_job);
        WORKERS.lock().unwrap().as_mut().unwrap().clear();
        drop(cold_queue);
        // Positive control: this guest really retains input when an Instance
        // is reused, reproducing the old host behavior rather than testing an
        // inert fixture that could never leak.
        let engine = runtime(&host_dir).unwrap();
        let program = engine.load(&wat::parse_str(STATE_GUEST).unwrap()).unwrap();
        let mut reused = engine.instantiate(&program).unwrap();
        reused.call("remember", b"private fixture input").unwrap();
        assert_eq!(
            reused.call("recall", b"").unwrap(),
            b"private fixture input"
        );
        drop(reused);
        for method in ["remember", "remember_fail"] {
            let result = request(
                "os.wasmstate",
                &format!("wasm.{method}"),
                json!("private fixture input"),
                &host_dir,
            );
            assert_eq!(result.is_ok(), method == "remember");
            let recalled = request("os.wasmstate", "wasm.recall", json!(null), &host_dir).unwrap();
            assert_eq!(
                recalled,
                json!({"text":""}),
                "no successful or failed call retains private input"
            );
        }

        let mut lab = None;
        let (heap, job) = captured_job("os.wasmstate", "recall", &host_dir);
        octosense_appstore::services::cancel_heap(heap);
        assert!(process("os.wasmstate", &mut lab, &job)
            .unwrap_err()
            .contains("cancelled"));
        assert!(
            lab.is_none(),
            "cancelled queued call never loads or executes guest code"
        );
        let (heap, mut job) = captured_job("os.wasmstate", "recall", &host_dir);
        job.deadline = Instant::now();
        assert!(process("os.wasmstate", &mut lab, &job)
            .unwrap_err()
            .contains("deadline"));
        assert!(lab.is_none());
        octosense_appstore::services::cancel_heap(heap);

        let (heap, stale) = captured_job("os.wasmstate", "recall", &host_dir);
        ship_state("os.wasmstate", "2.0.0", true);
        assert!(process("os.wasmstate", &mut lab, &stale)
            .unwrap_err()
            .contains("admission changed"));
        assert!(lab.is_none(), "old queued revision cannot run after update");
        octosense_appstore::services::cancel_heap(heap);
        let (heap, current) = captured_job("os.wasmstate", "recall", &host_dir);
        assert_eq!(
            process("os.wasmstate", &mut lab, &current).unwrap(),
            json!({"text":"new"}),
            "new revision executes its new bytes"
        );
        assert!(lab.is_some());
        octosense_appstore::services::cancel_heap(heap);

        // Real registration/grants change exactly between calculation and
        // delivery, so this fails if process omits its completion admission check.
        let (heap, in_flight) = captured_job("os.wasmstate", "recall", &host_dir);
        let error = process_checked("os.wasmstate", &mut lab, &in_flight, || {
            ship_state("os.wasmstate", "3.0.0", false)
        })
        .unwrap_err();
        assert!(error.contains("not granted"), "{error}");
        assert!(
            lab.is_none(),
            "revocation drops cached Programs before delivery"
        );
        octosense_appstore::services::cancel_heap(heap);
        ship_state("os.wasmstate", "4.0.0", true);
        let (heap, in_flight) = captured_job("os.wasmstate", "recall", &host_dir);
        assert!(process_checked("os.wasmstate", &mut lab, &in_flight, || {
            octosense_appstore::services::cancel_heap(heap)
        })
        .is_err());
        assert!(octosense_appstore::services::take_replies_for(&[heap]).is_empty());

        // Exercise the real service enqueue path against a deliberately held
        // worker receiver: no timing-dependent slow guest is needed to fill it.
        let (tx, held) = sync_channel(MAX_QUEUED_PER_APP);
        WORKERS.lock().unwrap().as_mut().unwrap().insert(
            "os.wasmstate".into(),
            Worker {
                jobs: tx,
                retired: Arc::new(AtomicBool::new(false)),
                admission: Arc::new(AdmissionEpoch {
                    admitted: OnceLock::from(Ok(Admission::current("os.wasmstate").unwrap())),
                    obsolete: AtomicBool::new(false),
                }),
            },
        );
        let mut heaps = Vec::new();
        for i in 0..=MAX_QUEUED_PER_APP {
            let heap = NEXT_HEAP.fetch_add(1, Ordering::Relaxed);
            heaps.push(heap);
            octosense_appstore::services::dispatch(
                ServiceCall {
                    app_id: "os.wasmstate".into(),
                    service: "wasm.recall".into(),
                    args: json!({}),
                    from_sheet: false,
                    may_prompt: false,
                    host_dir: host_dir.clone(),
                },
                heap,
                1,
                &mut NoSheet,
            );
            let replies = octosense_appstore::services::take_replies_for(&[heap]);
            if i < MAX_QUEUED_PER_APP {
                assert!(replies.is_empty());
            } else {
                assert!(replies[0].2.as_ref().unwrap_err().contains("queue is full"));
            }
        }
        let queued = held.try_iter().collect::<Vec<_>>();
        assert_eq!(queued.len(), MAX_QUEUED_PER_APP);
        ship_state("os.wasmstate", "5.0.0", true);
        for job in &queued {
            assert!(
                process("os.wasmstate", &mut lab, job)
                    .unwrap_err()
                    .contains("admission changed"),
                "the actual dispatch queue retains its original verified revision"
            );
        }
        let retry_heap = NEXT_HEAP.fetch_add(1, Ordering::Relaxed);
        octosense_appstore::services::dispatch(
            ServiceCall {
                app_id: "os.wasmstate".into(),
                service: "wasm.recall".into(),
                args: json!(null),
                from_sheet: false,
                may_prompt: false,
                host_dir: host_dir.clone(),
            },
            retry_heap,
            1,
            &mut NoSheet,
        );
        let retry = held.try_recv().unwrap();
        assert!(!Arc::ptr_eq(&retry.admission, &queued[0].admission));
        assert_eq!(
            process("os.wasmstate", &mut lab, &retry).unwrap(),
            json!({"text":""}),
            "a fresh dispatched request re-admits the new revision on the same worker"
        );
        octosense_appstore::services::cancel_heap(retry_heap);
        drop(retry);
        for heap in heaps {
            octosense_appstore::services::cancel_heap(heap);
        }
        assert!(queued.iter().all(|job| !job.live()));
        drop(queued);
        let used = BUFFERED_BYTES.load(Ordering::Acquire);
        assert!(Reservation::acquire(MAX_BUFFERED_BYTES - used).is_ok());
        let all = Reservation::acquire(MAX_BUFFERED_BYTES - used).unwrap();
        assert!(Reservation::acquire(1).is_err());
        drop(all);
        assert_eq!(BUFFERED_BYTES.load(Ordering::Acquire), used);
        WORKERS.lock().unwrap().as_mut().unwrap().clear();
        // The global limit is enforced even when one more admitted app has
        // no existing queue. These receiver handles stand in for busy workers.
        let mut held_workers = Vec::new();
        for n in 0..MAX_WORKERS {
            let (jobs, queue) = sync_channel(MAX_QUEUED_PER_APP);
            held_workers.push(queue);
            WORKERS.lock().unwrap().as_mut().unwrap().insert(
                format!("busy-{n}"),
                Worker {
                    jobs,
                    // Closing workers still hold resources. They must not
                    // release a slot merely because retirement has started.
                    retired: Arc::new(AtomicBool::new(n == 0)),
                    admission: Arc::default(),
                },
            );
        }
        assert!(
            request("os.wasmstate", "wasm.recall", json!(null), &host_dir)
                .unwrap_err()
                .contains("workers are busy")
        );
        WORKERS.lock().unwrap().as_mut().unwrap().clear();
        drop(held_workers);
        request("os.wasmstate", "wasm.recall", json!(null), &host_dir).unwrap();
        let idle_deadline = Instant::now() + WORKER_IDLE + Duration::from_secs(2);
        while !WORKERS.lock().unwrap().as_ref().unwrap().is_empty()
            && Instant::now() < idle_deadline
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            WORKERS.lock().unwrap().as_ref().unwrap().is_empty(),
            "idle workers release their cached modules and slot"
        );

        // A real signed store catalog withdrawal also vetoes an already
        // calculated result. This exercises installed_bundle/may_run, not a
        // fake admission checker or the system-app shortcut.
        use octosense_app_hub::{Catalog, Entry, HubKey, Source, Status};
        let id = "org.example.wasm-isolation";
        let bundle = octosense_app_hub::installed_bundle_dir(&root, id);
        std::fs::create_dir_all(bundle.join("fns")).unwrap();
        std::fs::write(bundle.join("main.splash"), "Label {text: \"Fixture\"}").unwrap();
        std::fs::write(
            bundle.join("fns/state.wasm"),
            wat::parse_str(STATE_GUEST).unwrap(),
        )
        .unwrap();
        let mut manifest = octosense_app_contract::AppManifest::parse(&json!({
            "schema":1,"id":id,"version":"1.0.0","name":"Wasm isolation fixture",
            "capabilities":["wasm"],"integrity":{"bundle_blake3":octosense_app_contract::digest_dir(&bundle).unwrap()}
        }).to_string()).unwrap();
        let publisher = HubKey::generate();
        octosense_app_hub::sign_manifest(&publisher, &mut manifest, "fixture").unwrap();
        std::fs::write(
            bundle.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let anchor = HubKey::generate();
        let working = HubKey::generate();
        let certificate = anchor.certify(&working.public_hex()).unwrap();
        std::env::set_var("OCTOSENSE_HUB_ANCHOR", anchor.public_hex());
        let entry = Entry {
            manifest,
            listing: None,
            tools: vec![],
            artifact: "fixture".into(),
            publisher: "fixture".into(),
            publisher_key: publisher.public_hex(),
            source: Source {
                repository: String::new(),
                commit: String::new(),
            },
            status: Status::Offered,
            admitted: "2026-10-08".into(),
        };
        let mut catalog = Catalog::new(1, "2026-10-08", vec![entry]);
        let write_catalog = |catalog: &mut Catalog| {
            working.sign_catalog(catalog, &certificate).unwrap();
            std::fs::write(
                root.join("catalog.json"),
                serde_json::to_vec(catalog).unwrap(),
            )
            .unwrap();
        };
        write_catalog(&mut catalog);
        let (heap, in_flight) = captured_job(id, "recall", &host_dir);
        let mut installed_lab = None;
        let error = process_checked(id, &mut installed_lab, &in_flight, || {
            catalog.sequence += 1;
            catalog.entries[0].status = Status::Withdrawn("fixture withdrawal".into());
            write_catalog(&mut catalog);
        })
        .unwrap_err();
        assert!(error.contains("withdrawn"), "{error}");
        assert!(installed_lab.is_none());
        assert!(Admission::current(id).is_err());
        octosense_appstore::services::cancel_heap(heap);
        let _ = std::fs::remove_dir_all(root);
    }

    /// The whole path, in a process of its own (the apps root and the
    /// registries are process-wide): Wasm Lab's tools reach its functions
    /// through the executor and the service, its script's requests do too,
    /// rogue code ends in an error and a fresh instance, and an app without
    /// the grant reaches nothing.
    #[test]
    fn the_apps_own_functions_answer_its_tools_and_its_script() {
        const CHILD: &str = "OCTOSENSE_TEST_WASM_SERVICE";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "wasm_service::tests::the_apps_own_functions_answer_its_tools_and_its_script",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let root =
            std::env::temp_dir().join(format!("octosense-wasm-service-{}", std::process::id()));
        let host_dir = root.join(".host");
        // This exact test runs alone in its child process. Keep the env
        // override and registered root identical even under an isolated suite.
        std::env::set_var("OCTOSENSE_APP_DATA", &root);
        octosense_appstore::set_data_root(root.clone());
        ship("os.wasmlab", "service", |_, _| {});
        // The same bundle without the capability (and so without its tools).
        ship("os.wasmplain", "plain", |dir, manifest| {
            manifest["capabilities"] = json!([]);
            manifest.as_object_mut().unwrap().remove("agent");
            std::fs::remove_file(dir.join("tools.json")).unwrap();
            std::fs::remove_file(dir.join("AGENT.md")).unwrap();
        });
        register();

        // An agent's tool call, as the relay hands it to the app's executor.
        let (_, bundle) = script_apps::admitted_bundle("os.wasmlab").unwrap();
        let loaded = script_apps::from_bundle(&bundle).unwrap();
        let executor = HostServiceExecutor {
            app: "os.wasmlab".into(),
            tools: loaded.host_service_tools,
            methods: loaded.host_methods,
            families: loaded.families,
            host_dir: host_dir.clone(),
        };
        let sent: Arc<Mutex<Vec<Value>>> = Arc::default();
        let into = sent.clone();
        let call = HostToolCall::parse(&json!({"peer": "p", "session_id": "s", "turn_id": "t", "call_id": "c1",
            "name": "wasmlab.find_slots",
            "args": {"day_start": "09:00", "day_end": "12:00", "duration": 30, "busy": [["09:30", "11:00"]]}}))
        .unwrap();
        executor.execute(
            call,
            ToolReply::new("c1", move |v| into.lock().unwrap().push(v)),
        );
        for _ in 0..1000 {
            script_apps::poll();
            if !sent.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let answer = sent.lock().unwrap().pop().expect("the tool answered");
        assert_eq!(answer["ok"], true, "{answer}");
        // Every 15 minutes, the default step.
        assert_eq!(
            answer["data"]["slots"],
            json!([
                ["09:00", "09:30"],
                ["11:00", "11:30"],
                ["11:15", "11:45"],
                ["11:30", "12:00"]
            ]),
            "{answer}"
        );

        // The script's own requests: a string goes in as its text, and an
        // output that is not JSON comes back as text.
        let html = request("os.wasmlab", "wasm.md_to_html", json!("# Hi"), &host_dir).unwrap();
        assert_eq!(html["text"], "<h1>Hi</h1>\n");
        let ranked = request(
            "os.wasmlab",
            "wasm.fuzzy_rank",
            json!({"query": "cal", "items": ["Mail", "Calendar"]}),
            &host_dir,
        )
        .unwrap();
        assert_eq!(ranked["ranked"][0]["item"], "Calendar");
        let error = request(
            "os.wasmlab",
            "wasm.find_slots",
            json!({"day_start": "12:00", "day_end": "09:00", "duration": 30, "busy": []}),
            &host_dir,
        )
        .unwrap_err();
        assert!(error.contains("ends before it starts"), "{error}");
        let error = request("os.wasmlab", "wasm.nothing", json!({}), &host_dir).unwrap_err();
        assert!(error.contains("no function"), "{error}");

        // Rogue code: an error each time, and the next call still answers.
        for mode in ["loop", "alloc", "panic", "recurse"] {
            let error =
                request("os.wasmlab", "wasm.rogue", json!({"mode": mode}), &host_dir).unwrap_err();
            assert!(
                error.contains("deadline") || error.contains("trapped"),
                "{mode}: {error}"
            );
            let ranked = request(
                "os.wasmlab",
                "wasm.fuzzy_rank",
                json!({"query": "m", "items": ["Mail"]}),
                &host_dir,
            )
            .unwrap();
            assert_eq!(ranked["ranked"][0]["item"], "Mail", "{mode}");
        }
        let described = request("os.wasmlab", "wasm.functions", json!({}), &host_dir).unwrap();
        assert_eq!(
            described["functions"],
            json!([
                "find_slots",
                "fuzzy_rank",
                "md_to_html",
                "rogue",
                "text_diff"
            ])
        );
        assert_eq!(described["modules"][0]["file"], "wasmlab.wasm");
        assert_eq!(described["modules"][0]["instance_policy"], "fresh-per-call");
        assert_eq!(described["stats"]["rogue"]["errors"], 4);
        assert!(
            host_dir
                .join("wasm-cache")
                .read_dir()
                .unwrap()
                .next()
                .is_some(),
            "compiled code is cached"
        );

        // No grant, no functions: the service checks the admitted manifest
        // itself, whoever dispatched the request.
        let error = request(
            "os.wasmplain",
            "wasm.fuzzy_rank",
            json!({"query": "m", "items": []}),
            &host_dir,
        )
        .unwrap_err();
        assert!(
            error.contains("was not granted the wasm service"),
            "{error}"
        );
        // And a sheet never reaches it.
        let heap = NEXT_HEAP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let call = ServiceCall {
            app_id: "os.wasmlab".into(),
            service: "wasm.functions".into(),
            args: json!({}),
            from_sheet: true,
            may_prompt: true,
            host_dir: host_dir.clone(),
        };
        octosense_appstore::services::dispatch(call, heap, 1, &mut NoSheet);
        let (_, _, answer) = octosense_appstore::services::take_replies_for(&[heap])
            .pop()
            .unwrap();
        assert!(answer.unwrap_err().contains("no sheet"));
        let _ = std::fs::remove_dir_all(root);
    }

    /// The spike's component (ADR 0014): `pulldown-cmark` and friends, built
    /// with plain cargo for `wasm32-wasip2`.
    const NOTES_COMPONENT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../wasm-host/tests/fixtures/notes.component.wasm"
    );

    /// A component's functions, in a process of its own (the area env and
    /// the registries are process-wide): they answer with typed JSON under
    /// either spelling of their names; one instance keeps its state between
    /// calls until a trap spends it; and its files are the app's own
    /// storage, under what is left of its quota, and absent without the
    /// storage capability.
    #[test]
    fn a_components_instance_keeps_state_and_its_files_stay_in_the_apps_storage() {
        const CHILD: &str = "OCTOSENSE_TEST_WASM_COMPONENT";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "wasm_service::tests::a_components_instance_keeps_state_and_its_files_stay_in_the_apps_storage",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        const APP: &str = "os.wasmnotes";
        const PLAIN: &str = "os.wasmplain";
        let root =
            std::env::temp_dir().join(format!("octosense-wasm-component-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bundle = root.join("bundle");
        std::fs::create_dir_all(bundle.join("fns")).unwrap();
        std::fs::copy(NOTES_COMPONENT, bundle.join("fns/notes.wasm")).unwrap();
        // The apps' storage as the shell keeps it: a 4 KiB ceiling for one,
        // no storage capability for the other.
        let storage = crate::app_storage::Storage::with_file_secrets(
            crate::app_storage::Layout::new(&root.join("home")).unwrap(),
        );
        let mut env = areas::FixedEnv {
            storage: Some(storage.clone()),
            ..areas::FixedEnv::default()
        };
        env.quotas.insert(
            APP.into(),
            areas::JailQuota {
                bytes: Some(4096),
                storage: true,
            },
        );
        env.quotas.insert(
            PLAIN.into(),
            areas::JailQuota {
                bytes: None,
                storage: false,
            },
        );
        *AREA_ENV.lock().unwrap() = Some(Arc::new(env));
        let host_dir = root.join(".host");
        let runtime = runtime(&host_dir).unwrap();
        let admission = Admission {
            root: root.clone(),
            bundle,
            manifest: json!({}),
        };
        let mut lab = Lab::from_bundle(APP, runtime, admission.clone(), || Ok(())).unwrap();
        let (_, reply) = pending_reply(APP, &host_dir);
        // What the app's script gets from `host.request("wasm." + method, args)`.
        let call = |lab: &mut Lab, method: &str, args: Value| {
            let text = matches!(args, Value::String(_));
            let input = input_bytes(&args).unwrap();
            lab.answer(
                method,
                &input,
                text,
                Instant::now() + REQUEST_TIMEOUT,
                reply.clone(),
            )
        };

        // Typed JSON: a bare value for one parameter, an object by name or
        // an array in order; a record comes back as an object.
        assert_eq!(
            call(&mut lab, "to_html", json!("# Hi")).unwrap(),
            "<h1>Hi</h1>\n"
        );
        assert_eq!(
            call(&mut lab, "to-html", json!({"markdown": "*a*"})).unwrap(),
            "<p><em>a</em></p>\n"
        );
        assert_eq!(
            call(&mut lab, "analyze", json!(["# One\n\ntwo words"])).unwrap(),
            json!({"words": 4, "lines": 3, "headings": ["One"]})
        );
        let error = call(&mut lab, "nothing", json!({})).unwrap_err();
        assert!(error.contains("no function"), "{error}");

        // One instance, kept between calls, until a trap spends it.
        assert_eq!(call(&mut lab, "count", json!({})).unwrap(), 1);
        assert_eq!(call(&mut lab, "count", Value::Null).unwrap(), 2);
        let error = lab
            .answer(
                "spin",
                b"{}",
                false,
                Instant::now() + Duration::from_millis(100),
                reply.clone(),
            )
            .unwrap_err();
        assert!(error.contains("deadline"), "{error}");
        assert_eq!(call(&mut lab, "count", json!({})).unwrap(), 1);
        let described = lab.describe();
        let notes = &described["modules"][0];
        assert_eq!(notes["kind"], "component", "{described}");
        assert_eq!(notes["instance_policy"], "kept-between-calls");
        assert_eq!(notes["instances"], 2);
        assert_eq!(notes["storage"], "app folder");
        assert!(
            notes["exports"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["name"] == "save_html" && e["wit"] == "save-html"),
            "{described}"
        );
        assert!(described["functions"]
            .as_array()
            .unwrap()
            .contains(&json!("echo_bytes")));
        assert_eq!(described["stats"]["spin"]["errors"], 1);

        // Its files are the app's own storage, the folder its script's fs.*
        // sees, and nothing outside it.
        let jail = storage.layout().app(APP).unwrap().jail;
        let saved = json!({"markdown": "# Hi", "path": "a.html"});
        assert_eq!(call(&mut lab, "save_html", saved.clone()).unwrap(), 12);
        assert_eq!(
            std::fs::read_to_string(jail.join("a.html")).unwrap(),
            "<h1>Hi</h1>\n"
        );
        assert_eq!(
            call(&mut lab, "read_file", json!("a.html")).unwrap(),
            "<h1>Hi</h1>\n"
        );
        std::fs::write(root.join("outside.txt"), "host").unwrap();
        for outside in ["../outside.txt", root.join("outside.txt").to_str().unwrap()] {
            assert!(
                call(&mut lab, "read_file", json!(outside)).is_err(),
                "{outside}"
            );
        }

        // The quota (4 KiB here): what is left of it is what a call may add.
        // A write past it fails inside the component, which keeps its
        // instance and state; a smaller one still goes in.
        let before = call(&mut lab, "count", json!({}))
            .unwrap()
            .as_u64()
            .unwrap();
        let big = json!({"markdown": "x".repeat(5000), "path": "big.html"});
        let error = call(&mut lab, "save_html", big).unwrap_err();
        assert!(error.ends_with("the storage budget is used up"), "{error}");
        assert_eq!(std::fs::metadata(jail.join("big.html")).unwrap().len(), 0);
        let small = json!({"markdown": "# Hi", "path": "b.html"});
        assert_eq!(call(&mut lab, "save_html", small).unwrap(), 12);
        // What the app writes otherwise counts too, from the next call on.
        std::fs::write(jail.join("filler"), vec![0u8; 4096 - 24]).unwrap();
        let error = call(
            &mut lab,
            "save_html",
            json!({"markdown": "# Hi", "path": "c.html"}),
        )
        .unwrap_err();
        assert!(error.ends_with("the storage budget is used up"), "{error}");
        assert_eq!(call(&mut lab, "count", json!({})).unwrap(), before + 1);
        let described = lab.describe();
        assert_eq!(described["modules"][0]["storage"], "app folder");
        assert_eq!(described["modules"][0]["storage_left"], 0);
        assert_eq!(described["modules"][0]["instances"], 2);

        // Without the storage capability, no folder at all.
        let mut plain = Lab::from_bundle(PLAIN, runtime, admission, || Ok(())).unwrap();
        let (_, plain_reply) = pending_reply(PLAIN, &host_dir);
        let error = plain
            .answer(
                "save_html",
                saved.to_string().as_bytes(),
                false,
                Instant::now() + REQUEST_TIMEOUT,
                plain_reply.clone(),
            )
            .unwrap_err();
        assert!(!error.is_empty());
        assert_eq!(plain.describe()["modules"][0]["storage"], "none");
        let _ = std::fs::remove_dir_all(root);
    }

    /// A component through the whole path, in a process of its own: shipped
    /// as a system app that requires `wasm-components-v1` (App Hub's
    /// admission reads the component's imports), its functions answer the
    /// app's script with typed JSON, its instance keeps its state from one
    /// request to the next, and without `storage` it has no folder.
    #[test]
    fn a_shipped_component_answers_its_apps_script() {
        const CHILD: &str = "OCTOSENSE_TEST_WASM_SHIPPED_COMPONENT";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "wasm_service::tests::a_shipped_component_answers_its_apps_script",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "octosense-wasm-shipped-component-{}",
            std::process::id()
        ));
        let host_dir = root.join(".host");
        // This exact test runs alone in its child process. Keep the env
        // override and registered root identical even under an isolated suite.
        std::env::set_var("OCTOSENSE_APP_DATA", &root);
        octosense_appstore::set_data_root(root.clone());
        ship("os.wasmnotes", "component", |dir, manifest| {
            manifest["capabilities"] = json!(["wasm"]);
            manifest["requires"] = json!(["wasm-components-v1"]);
            manifest.as_object_mut().unwrap().remove("agent");
            std::fs::remove_file(dir.join("tools.json")).unwrap();
            std::fs::remove_file(dir.join("AGENT.md")).unwrap();
            std::fs::remove_file(dir.join("fns/wasmlab.wasm")).unwrap();
            std::fs::copy(NOTES_COMPONENT, dir.join("fns/notes.wasm")).unwrap();
        });
        register();
        let call = |method: &str, args: Value| request("os.wasmnotes", method, args, &host_dir);

        assert_eq!(
            call("wasm.to_html", json!("# Hi")).unwrap(),
            "<h1>Hi</h1>\n"
        );
        assert_eq!(
            call("wasm.analyze", json!({"markdown": "# One\n\ntwo words"})).unwrap(),
            json!({"words": 4, "lines": 3, "headings": ["One"]})
        );
        assert_eq!(call("wasm.count", json!({})).unwrap(), 1);
        assert_eq!(call("wasm.count", json!({})).unwrap(), 2);
        let error = call(
            "wasm.save_html",
            json!({"markdown": "# Hi", "path": "a.html"}),
        )
        .unwrap_err();
        assert!(!error.is_empty());
        let described = call("wasm.functions", json!({})).unwrap();
        let notes = &described["modules"][0];
        assert_eq!(notes["file"], "notes.wasm", "{described}");
        assert_eq!(notes["kind"], "component");
        assert_eq!(notes["instances"], 1);
        assert_eq!(notes["storage"], "none");
        assert_eq!(described["stats"]["count"]["calls"], 2);
        let _ = std::fs::remove_dir_all(root);
    }

    /// A component's requests reach any host: an app's network
    /// declarations are shown at install, not enforced (the ruling of
    /// 8 October 2026), so an app with neither `net` nor `network.hosts`
    /// reaches a server on this device.
    #[test]
    fn a_components_requests_reach_any_host() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    if std::io::Read::read(&mut stream, &mut byte).unwrap_or(0) == 0 {
                        break;
                    }
                    request.push(byte[0]);
                }
                let _ = std::io::Write::write_all(
                    &mut stream,
                    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
                );
            }
        });
        let root = std::env::temp_dir().join(format!(
            "octosense-wasm-component-net-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let bundle = root.join("bundle");
        std::fs::create_dir_all(bundle.join("fns")).unwrap();
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../wasm-host/tests/fixtures/fetch.component.wasm"
            ),
            bundle.join("fns/fetch.wasm"),
        )
        .unwrap();
        let dir = script_apps::tests::stamped_bundle("wasmlab", "net", |_, _| {});
        let mut manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        manifest["requires"] = json!(["wasm-components-v1"]);
        manifest["capabilities"] = json!(["wasm"]);
        assert!(manifest.get("network").is_none());
        let host_dir = root.join(".host");
        let runtime = runtime(&host_dir).unwrap();
        let admission = Admission {
            root: root.clone(),
            bundle: bundle.clone(),
            manifest,
        };
        let mut lab = Lab::from_bundle("os.wasmnet", runtime, admission, || Ok(())).unwrap();
        for host in ["127.0.0.1", "localhost"] {
            let (_, reply) = pending_reply("os.wasmnet", &host_dir);
            let url = json!(format!("http://{host}:{port}/hi"));
            let answer = lab.answer(
                "get",
                url.to_string().as_bytes(),
                false,
                Instant::now() + REQUEST_TIMEOUT,
                reply,
            );
            assert_eq!(answer.unwrap(), "200 hello", "{host}");
        }
        assert_eq!(lab.describe()["modules"][0]["network"], json!(true));
        let _ = std::fs::remove_dir_all(root);
    }

    /// A component's `octosense:host` calls reach its app's granted host
    /// services, dispatched on the UI thread as its script's would be (a
    /// thread here plays the UI's part); nothing else, and never `wasm.*`.
    #[test]
    fn a_components_host_calls_reach_only_its_apps_granted_services() {
        struct Echo;
        impl HostService for Echo {
            fn family(&self) -> &'static str {
                "wasmhostecho"
            }
            fn call(&mut self, call: ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
                reply.send(Ok(json!({"app": call.app_id, "method": call.method(), "args": call.args, "may_prompt": call.may_prompt})));
            }
        }
        octosense_appstore::services::register_host_service(Box::new(Echo));
        let stop = Arc::new(AtomicBool::new(false));
        let pumping = stop.clone();
        let pump = std::thread::spawn(move || {
            while !pumping.load(Ordering::Relaxed) {
                pump_host_calls();
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let root = std::env::temp_dir().join(format!(
            "octosense-wasm-component-host-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let bundle = root.join("bundle");
        std::fs::create_dir_all(bundle.join("fns")).unwrap();
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../wasm-host/tests/fixtures/hostcall.component.wasm"
            ),
            bundle.join("fns/hostcall.wasm"),
        )
        .unwrap();
        let host_dir = root.join(".host");
        let admission = Admission {
            root: root.clone(),
            bundle,
            manifest: json!({"capabilities": ["wasm", "wasmhostecho"]}),
        };
        let mut lab = Lab::from_bundle(
            "org.example.hostcalls",
            runtime(&host_dir).unwrap(),
            admission,
            || Ok(()),
        )
        .unwrap();
        let mut call = |service: &str, args: &str| {
            let (_, reply) = pending_reply("org.example.hostcalls", &host_dir);
            lab.answer(
                "call",
                json!([service, args]).to_string().as_bytes(),
                false,
                Instant::now() + REQUEST_TIMEOUT,
                reply,
            )
        };
        let answer: Value = serde_json::from_str(
            call("wasmhostecho.get", r#"{"id":1}"#)
                .unwrap()
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            answer,
            json!({"app": "org.example.hostcalls", "method": "get", "args": {"id": 1}, "may_prompt": false})
        );
        let error = call("mail.list", "{}").unwrap_err();
        assert!(
            error.contains("was not granted the mail service"),
            "{error}"
        );
        let error = call("wasm.functions", "{}").unwrap_err();
        assert!(error.contains("cannot call wasm.*"), "{error}");
        let error = call("wasmhostecho.get", "not json").unwrap_err();
        assert!(error.contains("not JSON"), "{error}");
        stop.store(true, Ordering::Relaxed);
        pump.join().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    /// An installed app's functions are compiled before its first call, in
    /// a process of its own (the apps root and the registries are
    /// process-wide): the first call loads them from the cache.
    #[test]
    fn an_installed_apps_first_call_loads_from_the_cache() {
        const CHILD: &str = "OCTOSENSE_TEST_WASM_WARM";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "wasm_service::tests::an_installed_apps_first_call_loads_from_the_cache",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let root = std::env::temp_dir().join(format!("octosense-wasm-warm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let host_dir = root.join(".host");
        // This exact test runs alone in its child process. Keep the env
        // override and registered root identical even under an isolated suite.
        std::env::set_var("OCTOSENSE_APP_DATA", &root);
        octosense_appstore::set_data_root(root.clone());
        ship("os.wasmlab", "warm", |_, _| {});
        register();
        warm("os.wasmlab");
        let cache = host_dir.join("wasm-cache");
        let cached = || {
            std::fs::read_dir(&cache)
                .map(|entries| {
                    entries
                        .flatten()
                        .any(|e| e.path().extension().is_some_and(|x| x == "cwasm"))
                })
                .unwrap_or(false)
        };
        for _ in 0..3000 {
            if cached() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(cached(), "warm compiled nothing into {}", cache.display());
        let ranked = request(
            "os.wasmlab",
            "wasm.fuzzy_rank",
            json!({"query": "m", "items": ["Mail"]}),
            &host_dir,
        )
        .unwrap();
        assert_eq!(ranked["ranked"][0]["item"], "Mail");
        let described = request("os.wasmlab", "wasm.functions", json!({}), &host_dir).unwrap();
        assert_eq!(described["modules"][0]["from_cache"], true, "{described}");
        // Warming again finds it compiled.
        assert_eq!(warm_now("os.wasmlab").unwrap().0, 0);
        let _ = std::fs::remove_dir_all(root);
    }
}
