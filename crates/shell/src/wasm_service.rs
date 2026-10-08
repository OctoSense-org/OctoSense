//! The `wasm` service (ADR 0011, feature `wasm-lab`): an app's own
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

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use octosense_appstore::services::{HostService, Replier, ServiceCall, ServiceHost};
use octosense_wasm_host::{Limits, Program, Runtime};
use serde_json::{json, Value};

use crate::host_tools::script_apps;

const MAX_MODULES: usize = 8;
const MAX_WORKERS: usize = 4;
const MAX_QUEUED_PER_APP: usize = 4;
const MAX_INPUT_BYTES: usize = 1 << 20;
const MAX_BUFFERED_BYTES: usize = 16 << 20;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const WORKER_IDLE: Duration = Duration::from_secs(5);

static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
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
                return Err(std::io::Error::other("wasm input exceeds 1 MiB"));
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
            return Err("wasm input exceeds 1 MiB".into());
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
        let job = match jobs.recv_timeout(WORKER_IDLE) {
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
    let result =
        lab.as_mut()
            .unwrap()
            .answer(&job.method, &job.input, job.deadline, job.reply.clone());
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
}

struct Module {
    file: String,
    bytes: usize,
    load_ms: f64,
    from_cache: bool,
    program: Program,
    /// Diagnostic high-water mark only: guest memory is never retained.
    memory_bytes: usize,
    invocations: u64,
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
        let admission = job.check(app)?;
        let bundle = &admission.bundle;
        let runtime = runtime(&job.host_dir)?;
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
            admission: admission.clone(),
        };
        for path in files {
            job.check(app)?;
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
            let program = runtime.load(&bytes).map_err(|e| format!("{file}: {e}"))?;
            let load_ms = started.elapsed().as_secs_f64() * 1e3;
            for function in program.functions() {
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
            let from_cache = program.from_cache();
            makepad_widgets::log!(
                "wasm {app}: {file} ({} KiB) {} in {load_ms:.1} ms: {}",
                bytes.len() / 1024,
                if from_cache {
                    "loaded from the cache"
                } else {
                    "compiled"
                },
                program.functions().join(", ")
            );
            lab.modules.push(Module {
                file,
                bytes: bytes.len(),
                load_ms,
                from_cache,
                program,
                memory_bytes: 0,
                invocations: 0,
            });
        }
        Ok(lab)
    }

    fn answer(
        &mut self,
        method: &str,
        input: &[u8],
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
        let Some(&index) = self.owner.get(method) else {
            return Err(format!("{} has no function {method:?}", self.app));
        };
        let module = &mut self.modules[index];
        let started = Instant::now();
        let mut instance = self
            .runtime
            .instantiate_guarded(&module.program, deadline, move || reply.is_pending())
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

    fn describe(&self) -> Value {
        let modules: Vec<Value> = self
            .modules
            .iter()
            .map(|m| {
                json!({"file": m.file, "bytes": m.bytes, "load_ms": round(m.load_ms),
                    "from_cache": m.from_cache, "memory_bytes": m.memory_bytes, "invocations": m.invocations, "renewed": m.invocations.saturating_sub(1), "instance_policy": "fresh-per-call"})
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

    fn captured_job(app: &str, method: &str, host_dir: &Path) -> (usize, Job) {
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
        (
            heap,
            Job {
                method: method.into(),
                input: Vec::new(),
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
}
