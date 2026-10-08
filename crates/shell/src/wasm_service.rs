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
//! **Where it runs.** Requests arrive on the UI thread. Each app gets one
//! worker thread holding its instances, so a slow function holds up only
//! its own app. Compiled code is cached in the host directory
//! (`wasm-cache`), keyed by the module's digest. An instance that trapped or
//! ran past its deadline is replaced before the next call.
//!
//! Not done yet: an app updated while the shell runs keeps its old
//! functions until the shell restarts, and a worker lives as long as the
//! shell.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, SendError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use octosense_appstore::services::{HostService, Replier, ServiceCall, ServiceHost};
use octosense_wasm_host::{Instance, Limits, Program, Runtime};
use serde_json::{json, Value};

use crate::host_tools::script_apps;

/// The most modules one app's bundle may carry.
const MAX_MODULES: usize = 8;

/// One engine, one epoch ticker and one code cache for the process.
static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();

/// Each app's worker, by app id.
static WORKERS: Mutex<Option<HashMap<String, Sender<Job>>>> = Mutex::new(None);

struct Job {
    method: String,
    args: Value,
    host_dir: PathBuf,
    reply: Replier,
}

/// The `wasm` family for the Card runner and the tool executor.
pub struct WasmService;

impl HostService for WasmService {
    fn family(&self) -> &'static str {
        "wasm"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if call.from_sheet {
            reply.send(Err("the wasm service has no sheet".into()));
            return;
        }
        let app = call.app_id.clone();
        let job = Job { method: call.method().to_string(), host_dir: call.host_dir, args: call.args, reply };
        let mut workers = WORKERS.lock().unwrap_or_else(|e| e.into_inner());
        let workers = workers.get_or_insert_with(HashMap::new);
        // A worker that ended (only a panic ends one) is replaced.
        let job = match workers.get(&app) {
            Some(jobs) => match jobs.send(job) {
                Ok(()) => return,
                Err(SendError(job)) => job,
            },
            None => job,
        };
        let (jobs, queue) = channel();
        let name = app.clone();
        let spawned = std::thread::Builder::new().name(format!("wasm {app}")).spawn(move || work(name, queue));
        if let Err(error) = spawned {
            job.reply.send(Err(format!("cannot start {app}'s functions: {error}")));
            return;
        }
        let _ = jobs.send(job);
        workers.insert(app, jobs);
    }
}

pub fn register() {
    octosense_appstore::services::register_host_service(Box::new(WasmService));
}

/// One app's worker: loads its modules on the first request (again on the
/// next one, if that failed), then answers each request in turn.
fn work(app: String, jobs: Receiver<Job>) {
    let mut lab: Option<Lab> = None;
    for job in jobs {
        if lab.is_none() {
            match Lab::load(&app, &job.host_dir) {
                Ok(loaded) => lab = Some(loaded),
                Err(error) => {
                    makepad_widgets::log!("wasm {app}: {error}");
                    job.reply.send(Err(error));
                    continue;
                }
            }
        }
        if let Some(lab) = lab.as_mut() {
            job.reply.send(lab.answer(&job.method, &job.args));
        }
    }
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
}

struct Module {
    file: String,
    bytes: usize,
    load_ms: f64,
    from_cache: bool,
    program: Program,
    instance: Instance,
    /// Fresh instances made after a trap or a deadline.
    renewed: u32,
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
    fn load(app: &str, host_dir: &Path) -> Result<Lab, String> {
        let (_, bundle) = script_apps::admitted_bundle(app)?;
        if !script_apps::from_bundle(&bundle)?.families.contains("wasm") {
            return Err(format!("{app} was not granted the wasm service"));
        }
        let runtime = runtime(host_dir)?;
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
            return Err(format!("{app}'s bundle must carry 1 to {MAX_MODULES} fns/*.wasm modules"));
        }
        let mut lab = Lab { app: app.to_string(), runtime, modules: Vec::new(), owner: BTreeMap::new(), stats: BTreeMap::new() };
        for path in files {
            let file = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let bytes = std::fs::read(&path).map_err(|e| format!("{file}: {e}"))?;
            let started = Instant::now();
            let program = runtime.load(&bytes).map_err(|e| format!("{file}: {e}"))?;
            let instance = runtime.instantiate(&program).map_err(|e| format!("{file}: {e}"))?;
            let load_ms = started.elapsed().as_secs_f64() * 1e3;
            for function in program.functions() {
                if lab.owner.insert(function.clone(), lab.modules.len()).is_some() {
                    return Err(format!("{file}: {function} is exported by another module too"));
                }
            }
            let from_cache = program.from_cache();
            makepad_widgets::log!(
                "wasm {app}: {file} ({} KiB) {} in {load_ms:.1} ms: {}",
                bytes.len() / 1024,
                if from_cache { "loaded from the cache" } else { "compiled" },
                program.functions().join(", ")
            );
            lab.modules.push(Module { file, bytes: bytes.len(), load_ms, from_cache, program, instance, renewed: 0 });
        }
        Ok(lab)
    }

    fn answer(&mut self, method: &str, args: &Value) -> Result<Value, String> {
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
        if module.instance.spent() {
            module.instance = self.runtime.instantiate(&module.program).map_err(|e| e.to_string())?;
            module.renewed += 1;
        }
        let input = match args {
            Value::String(text) => text.clone().into_bytes(),
            other => serde_json::to_vec(other).map_err(|e| e.to_string())?,
        };
        let started = Instant::now();
        let result = module.instance.call(method, &input);
        let us = started.elapsed().as_secs_f64() * 1e6;
        for line in module.instance.take_logs() {
            makepad_widgets::log!("wasm {}: {line}", self.app);
        }
        let stats = self.stats.entry(method.to_string()).or_default();
        stats.calls += 1;
        stats.total_us += us;
        stats.max_us = stats.max_us.max(us);
        let output = result.map_err(|error| {
            stats.errors += 1;
            error.to_string()
        })?;
        Ok(serde_json::from_slice(&output).unwrap_or_else(|_| json!({"text": String::from_utf8_lossy(&output)})))
    }

    fn describe(&self) -> Value {
        let modules: Vec<Value> = self
            .modules
            .iter()
            .map(|m| {
                json!({"file": m.file, "bytes": m.bytes, "load_ms": round(m.load_ms),
                    "from_cache": m.from_cache, "memory_bytes": m.instance.memory_bytes(), "renewed": m.renewed})
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
        let names: Vec<&str> = loaded.tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["wasmlab.find_slots", "wasmlab.rank", "wasmlab.diff"]);
        assert_eq!(loaded.host_methods["wasmlab.find_slots"], "wasm.find_slots");
        assert_eq!(loaded.host_methods["wasmlab.rank"], "wasm.fuzzy_rank");
        assert_eq!(loaded.host_methods["wasmlab.diff"], "wasm.text_diff");
        assert!(loaded.tools.iter().all(|t| t["risk"] == "read" && t["output_schema"]["type"] == "object"));
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
        let call = ServiceCall { app_id: app.into(), service: service.into(), args, from_sheet: false, may_prompt: true, host_dir: host_dir.into() };
        octosense_appstore::services::dispatch(call, heap, 1, &mut NoSheet);
        for _ in 0..1000 {
            if let Some((_, _, answer)) = octosense_appstore::services::take_replies_for(&[heap]).pop() {
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
                .args(["--exact", "wasm_service::tests::the_apps_own_functions_answer_its_tools_and_its_script", "--nocapture"])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            return;
        }
        let root = std::env::temp_dir().join(format!("octosense-wasm-service-{}", std::process::id()));
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
        executor.execute(call, ToolReply::new("c1", move |v| into.lock().unwrap().push(v)));
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
        assert_eq!(answer["data"]["slots"], json!([["09:00", "09:30"], ["11:00", "11:30"], ["11:15", "11:45"], ["11:30", "12:00"]]), "{answer}");

        // The script's own requests: a string goes in as its text, and an
        // output that is not JSON comes back as text.
        let html = request("os.wasmlab", "wasm.md_to_html", json!("# Hi"), &host_dir).unwrap();
        assert_eq!(html["text"], "<h1>Hi</h1>\n");
        let ranked = request("os.wasmlab", "wasm.fuzzy_rank", json!({"query": "cal", "items": ["Mail", "Calendar"]}), &host_dir).unwrap();
        assert_eq!(ranked["ranked"][0]["item"], "Calendar");
        let error = request("os.wasmlab", "wasm.find_slots", json!({"day_start": "12:00", "day_end": "09:00", "duration": 30, "busy": []}), &host_dir).unwrap_err();
        assert!(error.contains("ends before it starts"), "{error}");
        let error = request("os.wasmlab", "wasm.nothing", json!({}), &host_dir).unwrap_err();
        assert!(error.contains("no function"), "{error}");

        // Rogue code: an error each time, and the next call still answers.
        for mode in ["loop", "alloc", "panic", "recurse"] {
            let error = request("os.wasmlab", "wasm.rogue", json!({"mode": mode}), &host_dir).unwrap_err();
            assert!(error.contains("deadline") || error.contains("trapped"), "{mode}: {error}");
            let ranked = request("os.wasmlab", "wasm.fuzzy_rank", json!({"query": "m", "items": ["Mail"]}), &host_dir).unwrap();
            assert_eq!(ranked["ranked"][0]["item"], "Mail", "{mode}");
        }
        let described = request("os.wasmlab", "wasm.functions", json!({}), &host_dir).unwrap();
        assert_eq!(described["functions"], json!(["find_slots", "fuzzy_rank", "md_to_html", "rogue", "text_diff"]));
        assert_eq!(described["modules"][0]["file"], "wasmlab.wasm");
        assert_eq!(described["modules"][0]["renewed"], 4);
        assert_eq!(described["stats"]["rogue"]["errors"], 4);
        assert!(host_dir.join("wasm-cache").read_dir().unwrap().next().is_some(), "compiled code is cached");

        // No grant, no functions: the service checks the admitted manifest
        // itself, whoever dispatched the request.
        let error = request("os.wasmplain", "wasm.fuzzy_rank", json!({"query": "m", "items": []}), &host_dir).unwrap_err();
        assert!(error.contains("was not granted the wasm service"), "{error}");
        // And a sheet never reaches it.
        let heap = NEXT_HEAP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let call = ServiceCall { app_id: "os.wasmlab".into(), service: "wasm.functions".into(), args: json!({}), from_sheet: true, may_prompt: true, host_dir: host_dir.clone() };
        octosense_appstore::services::dispatch(call, heap, 1, &mut NoSheet);
        let (_, _, answer) = octosense_appstore::services::take_replies_for(&[heap]).pop().unwrap();
        assert!(answer.unwrap_err().contains("no sheet"));
        let _ = std::fs::remove_dir_all(root);
    }
}
