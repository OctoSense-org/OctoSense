//! Developers' own functions, compiled to WebAssembly, run for their app in
//! Wasmtime (Cranelift): light computing and algorithms an app ships beside
//! its script (ADR 0011).
//!
//! # The guest ABI
//!
//! A module is core WebAssembly (the guest crate `octosense-guest` writes all
//! of this for a Rust function):
//! - it exports `memory`, `octo_alloc(len: i32) -> i32` and
//!   `octo_free(ptr: i32, len: i32)`;
//! - every other export of type `(i32, i32) -> i64` is a function the app may
//!   call: it takes its input bytes at `(ptr, len)` and returns
//!   `(out_ptr << 32) | out_len` of a buffer it allocated, which holds a
//!   status byte (`0` ok, `1` error) and then the output, or the error text;
//! - its only import may be `octo.log(ptr: i32, len: i32)`.
//!
//! # Containment
//!
//! A module reaches nothing but its own memory and `octo.log`: no file,
//! network, clock or other import exists, and a module that asks for one is
//! refused when it loads. Every call has a deadline (Wasmtime's epoch
//! interruption), the memory and the wasm stack have caps, and input and
//! output have a size limit. A trap, a deadline or a limit ends the call
//! with an error; it never ends the process. The instance is spent then
//! (its module's state may be half-updated) and refuses further calls; a
//! fresh instance of the same program costs a fraction of a millisecond.

use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub mod component;

use sha2::{Digest, Sha256};
use wasmtime::{
    Caller, Config, Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, Trap,
    TypedFunc, ValType,
};

/// How often the epoch advances: the granularity of a deadline.
const TICK: Duration = Duration::from_millis(10);

/// The most one `octo.log` line may hold; longer ones are cut.
const LOG_LINE: usize = 1024;

/// The most log lines an instance keeps until they are read.
const LOG_LINES: usize = 64;

/// What every instance of a [`Runtime`] may use.
#[derive(Clone, Debug)]
pub struct Limits {
    /// How long one call may run.
    pub deadline: Duration,
    /// How long one call may run when it may reach the network (a
    /// component granted hosts): it waits for replies.
    pub network_deadline: Duration,
    /// The most linear memory an instance may have.
    pub memory_bytes: usize,
    /// The most elements in an instance's single table (including growth).
    pub table_elements: usize,
    /// The largest module the runtime loads.
    pub module_bytes: usize,
    /// The largest input, and the largest output, of a call.
    pub io_bytes: usize,
    /// The wasm stack of a call.
    pub stack_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            deadline: Duration::from_secs(2),
            network_deadline: Duration::from_secs(10),
            memory_bytes: 256 << 20,
            table_elements: 16_384,
            module_bytes: 8 << 20,
            io_bytes: 16 << 20,
            stack_bytes: 512 << 10,
        }
    }
}

/// Why a module cannot load.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// It is bigger than [`Limits::module_bytes`].
    TooLarge(usize),
    /// It is not valid WebAssembly for this engine.
    Invalid(String),
    /// It imports something other than `octo.log`.
    Import(String),
    /// It lacks `memory`, `octo_alloc` or `octo_free`, or one has the wrong type.
    Export(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::TooLarge(n) => write!(f, "the module is {n} bytes, over the limit"),
            LoadError::Invalid(why) => write!(f, "not a valid module: {why}"),
            LoadError::Import(what) => {
                write!(f, "the module imports {what}; only octo.log is provided")
            }
            LoadError::Export(what) => write!(f, "the module does not export {what}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Why a call failed.
#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    /// The module has no such function.
    NoSuchFunction(String),
    /// The call ran past [`Limits::deadline`].
    Deadline,
    /// Its input or output is bigger than [`Limits::io_bytes`].
    TooLarge(usize),
    /// The function trapped: a panic, an out-of-bounds access, a stack
    /// overflow, memory over its cap. The text says which.
    Trap(String),
    /// The function returned an error of its own.
    Guest(String),
    /// An earlier call on this instance trapped or ran past its deadline, so
    /// it takes no more calls: instantiate the program again.
    Spent,
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallError::NoSuchFunction(name) => write!(f, "no function {name}"),
            CallError::Deadline => write!(f, "the call ran past its deadline"),
            CallError::TooLarge(n) => write!(f, "{n} bytes is over the input/output limit"),
            CallError::Trap(why) => write!(f, "the function trapped: {why}"),
            CallError::Guest(why) => write!(f, "{why}"),
            CallError::Spent => write!(f, "the instance stopped after an earlier trap"),
        }
    }
}

impl std::error::Error for CallError {}

/// The engine and the limits every instance shares. Its epoch ticker runs
/// for as long as the runtime or any of its instances lives: an instance
/// never outlives its deadline.
pub struct Runtime {
    engine: Engine,
    limits: Limits,
    cache_dir: Option<PathBuf>,
    ticker: Arc<Ticker>,
}

/// A loaded module: checked, compiled (or taken from the cache).
#[derive(Clone)]
pub struct Program {
    module: Module,
    functions: Vec<String>,
    from_cache: bool,
}

impl Program {
    /// The functions an app may call, sorted.
    pub fn functions(&self) -> &[String] {
        &self.functions
    }

    /// Whether the compiled code came from the cache.
    pub fn from_cache(&self) -> bool {
        self.from_cache
    }
}

struct State {
    limits: StoreLimits,
    logs: Vec<String>,
}

struct InvocationGuard {
    deadline: Instant,
    pending: Box<dyn Fn() -> bool + Send + Sync>,
}
impl InvocationGuard {
    fn live(&self) -> bool {
        Instant::now() < self.deadline && (self.pending)()
    }
}

/// One app's instance of a [`Program`]: its own memory and its own state.
/// It may move to another thread.
pub struct Instance {
    store: Store<State>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    free: TypedFunc<(i32, i32), ()>,
    functions: HashMap<String, TypedFunc<(i32, i32), i64>>,
    deadline_ticks: u64,
    io_bytes: usize,
    /// A call trapped or ran past its deadline. The module's own state may
    /// be half-updated then (a Rust guest's stack pointer and allocator live
    /// in its memory and globals), so the instance takes no more calls.
    spent: bool,
    guard: Option<Arc<InvocationGuard>>,
    _ticker: Arc<Ticker>,
}

impl Runtime {
    /// A runtime with these limits. Compiled code is cached in `cache_dir`
    /// when one is given (keyed by the module's digest and the engine).
    pub fn new(limits: Limits, cache_dir: Option<PathBuf>) -> Result<Runtime, String> {
        let mut config = Config::new();
        config.epoch_interruption(true);
        // Components (ADR 0014) share the engine, its epoch and its cache.
        config.wasm_component_model(true);
        config.max_wasm_stack(limits.stack_bytes);
        let engine = Engine::new(&config).map_err(|e| format!("{e:#}"))?;
        let ticker = Arc::new(Ticker::start(engine.clone()));
        Ok(Runtime {
            engine,
            limits,
            cache_dir,
            ticker,
        })
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Checks and compiles `bytes`, or takes its compiled code from the cache.
    pub fn load(&self, bytes: &[u8]) -> Result<Program, LoadError> {
        if bytes.len() > self.limits.module_bytes {
            return Err(LoadError::TooLarge(bytes.len()));
        }
        let cached = self.cache_path(bytes);
        let mut from_cache = false;
        let module = match cached.as_ref().and_then(|path| std::fs::read(path).ok()) {
            // SAFETY: the cache holds only what `Module::serialize` wrote for
            // this engine (the key includes its compatibility hash), in a
            // directory that belongs to the shell.
            Some(code) => match unsafe { Module::deserialize(&self.engine, &code) } {
                Ok(module) => {
                    from_cache = true;
                    module
                }
                Err(_) => self.compile(bytes, cached.as_ref())?,
            },
            None => self.compile(bytes, cached.as_ref())?,
        };
        let functions = check(&module)?;
        Ok(Program {
            module,
            functions,
            from_cache,
        })
    }

    fn compile(&self, bytes: &[u8], cache: Option<&PathBuf>) -> Result<Module, LoadError> {
        let module =
            Module::new(&self.engine, bytes).map_err(|e| LoadError::Invalid(format!("{e:#}")))?;
        if let Some(path) = cache {
            // Check before caching: a refused module is never stored.
            check(&module)?;
            if let Ok(code) = module.serialize() {
                self.publish_cache(path, &code);
            }
        }
        Ok(module)
    }

    /// Writes compiled code to the cache atomically.
    fn publish_cache(&self, path: &PathBuf, code: &[u8]) {
        {
            {
                // Different apps may compile identical bytes concurrently.
                // Never truncate another worker's staging file: deserialization
                // is only safe for complete, unmodified serialized modules.
                static NEXT_CACHE_WRITE: AtomicU64 = AtomicU64::new(0);
                let tmp = path.with_extension(format!(
                    "{}-{}.tmp",
                    std::process::id(),
                    NEXT_CACHE_WRITE.fetch_add(1, Ordering::Relaxed)
                ));
                let published = (|| -> std::io::Result<()> {
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&tmp)?;
                    file.write_all(code)?;
                    drop(file);
                    std::fs::rename(&tmp, path)
                })();
                if published.is_ok() {
                    // Read it back off this thread: an on-access scanner
                    // (Microsoft Defender's, on a managed Mac) holds the
                    // first open of a new file for about a second, so let
                    // that happen now rather than at the next load.
                    let path = path.clone();
                    let _ = std::thread::Builder::new()
                        .name("wasm-cache".into())
                        .spawn(move || drop(std::fs::read(path)));
                }
                let _ = std::fs::remove_file(tmp);
            }
        }
    }

    fn cache_path(&self, bytes: &[u8]) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        std::fs::create_dir_all(dir).ok()?;
        let mut engine = std::collections::hash_map::DefaultHasher::new();
        self.engine
            .precompile_compatibility_hash()
            .hash(&mut engine);
        let digest = Sha256::digest(bytes);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        Some(dir.join(format!("{hex}-{:016x}.cwasm", engine.finish())))
    }

    /// A fresh instance of `program`, with its own memory.
    pub fn instantiate(&self, program: &Program) -> Result<Instance, LoadError> {
        self.instantiate_inner(program, None)
    }

    /// A fresh invocation whose start function and exported call share a
    /// deadline. Cancellation is checked before starting and every epoch;
    /// compiled code is reusable, but none of the instance's state is shared.
    pub fn instantiate_guarded(
        &self,
        program: &Program,
        deadline: Instant,
        pending: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Instance, LoadError> {
        let deadline = deadline.min(Instant::now() + self.limits.deadline);
        self.instantiate_inner(program, Some((deadline, Box::new(pending))))
    }

    fn instantiate_inner(
        &self,
        program: &Program,
        guard: Option<(Instant, Box<dyn Fn() -> bool + Send + Sync>)>,
    ) -> Result<Instance, LoadError> {
        if guard
            .as_ref()
            .is_some_and(|(deadline, pending)| Instant::now() >= *deadline || !pending())
        {
            return Err(LoadError::Invalid(
                "invocation cancelled or past its deadline".into(),
            ));
        }
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.limits.memory_bytes)
            .table_elements(self.limits.table_elements)
            .instances(1)
            .memories(1)
            .tables(1)
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(
            &self.engine,
            State {
                limits,
                logs: Vec::new(),
            },
        );
        store.limiter(|state| &mut state.limits);
        let guard =
            guard.map(|(deadline, pending)| Arc::new(InvocationGuard { deadline, pending }));
        if let Some(guard) = guard.clone() {
            store.epoch_deadline_callback(move |_| {
                Ok(if !guard.live() {
                    wasmtime::UpdateDeadline::Interrupt
                } else {
                    wasmtime::UpdateDeadline::Continue(1)
                })
            });
        } else {
            store.epoch_deadline_trap();
        }
        // Instantiation runs the start function and data initialisers: a
        // deadline covers it too.
        let deadline_ticks = self
            .limits
            .deadline
            .as_millis()
            .div_ceil(TICK.as_millis())
            .max(1) as u64;
        store.set_epoch_deadline(if guard.is_some() { 1 } else { deadline_ticks });
        let mut linker = Linker::new(&self.engine);
        linker
            .func_wrap(
                "octo",
                "log",
                |mut caller: Caller<'_, State>, ptr: i32, len: i32| {
                    let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory())
                    else {
                        return;
                    };
                    let len = (len.max(0) as usize).min(LOG_LINE);
                    let mut buf = vec![0u8; len];
                    if memory.read(&caller, ptr as u32 as usize, &mut buf).is_ok()
                        && caller.data().logs.len() < LOG_LINES
                    {
                        let line = String::from_utf8_lossy(&buf).into_owned();
                        caller.data_mut().logs.push(line);
                    }
                },
            )
            .map_err(|e| LoadError::Invalid(format!("{e:#}")))?;
        let instance = linker
            .instantiate(&mut store, &program.module)
            .map_err(|e| LoadError::Invalid(format!("{e:#}")))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| LoadError::Export("memory".into()))?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "octo_alloc")
            .map_err(|_| LoadError::Export("octo_alloc(i32) -> i32".into()))?;
        let free = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "octo_free")
            .map_err(|_| LoadError::Export("octo_free(i32, i32)".into()))?;
        let mut functions = HashMap::new();
        for name in &program.functions {
            let f = instance
                .get_typed_func::<(i32, i32), i64>(&mut store, name)
                .map_err(|_| LoadError::Export(format!("{name}(i32, i32) -> i64")))?;
            functions.insert(name.clone(), f);
        }
        Ok(Instance {
            store,
            memory,
            alloc,
            free,
            functions,
            deadline_ticks,
            io_bytes: self.limits.io_bytes,
            spent: false,
            guard,
            _ticker: self.ticker.clone(),
        })
    }
}

/// The imports and exports a module must have; returns its functions.
fn check(module: &Module) -> Result<Vec<String>, LoadError> {
    for import in module.imports() {
        let ok = import.module() == "octo"
            && import.name() == "log"
            && import.ty().func().is_some_and(|f| {
                f.params().map(|t| t.is_i32()).eq([true, true]) && f.results().len() == 0
            });
        if !ok {
            return Err(LoadError::Import(format!(
                "{}.{}",
                import.module(),
                import.name()
            )));
        }
    }
    let mut functions = Vec::new();
    let mut memory = false;
    let mut alloc = false;
    let mut free = false;
    for export in module.exports() {
        let ty = export.ty();
        match export.name() {
            "memory" => memory = ty.memory().is_some(),
            "octo_alloc" => {
                alloc = ty
                    .func()
                    .is_some_and(|f| sig(f.params(), f.results(), &[ValType::I32], &[ValType::I32]))
            }
            "octo_free" => {
                free = ty.func().is_some_and(|f| {
                    sig(f.params(), f.results(), &[ValType::I32, ValType::I32], &[])
                })
            }
            name => {
                if ty.func().is_some_and(|f| {
                    sig(
                        f.params(),
                        f.results(),
                        &[ValType::I32, ValType::I32],
                        &[ValType::I64],
                    )
                }) {
                    functions.push(name.to_string());
                }
            }
        }
    }
    for (have, what) in [
        (memory, "memory"),
        (alloc, "octo_alloc(i32) -> i32"),
        (free, "octo_free(i32, i32)"),
    ] {
        if !have {
            return Err(LoadError::Export(what.into()));
        }
    }
    functions.sort();
    Ok(functions)
}

fn sig(
    params: impl ExactSizeIterator<Item = ValType>,
    results: impl ExactSizeIterator<Item = ValType>,
    want_params: &[ValType],
    want_results: &[ValType],
) -> bool {
    let same = |a: ValType, b: &ValType| {
        matches!(
            (a, b),
            (ValType::I32, ValType::I32) | (ValType::I64, ValType::I64)
        )
    };
    params.len() == want_params.len()
        && results.len() == want_results.len()
        && params.zip(want_params).all(|(a, b)| same(a, b))
        && results.zip(want_results).all(|(a, b)| same(a, b))
}

impl Instance {
    /// The functions this instance has, sorted.
    pub fn functions(&self) -> Vec<String> {
        let mut names: Vec<String> = self.functions.keys().cloned().collect();
        names.sort();
        names
    }

    /// Calls `name` with `input` and returns its output. After a trap or a
    /// deadline the instance is spent ([`CallError::Spent`]): instantiate
    /// the program again, which is cheap. A function's own error leaves the
    /// instance as it was.
    pub fn call(&mut self, name: &str, input: &[u8]) -> Result<Vec<u8>, CallError> {
        if self.spent {
            return Err(CallError::Spent);
        }
        if self.guard.as_ref().is_some_and(|guard| !guard.live()) {
            self.spent = true;
            return Err(CallError::Deadline);
        }
        let f = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| CallError::NoSuchFunction(name.into()))?;
        if input.len() > self.io_bytes {
            return Err(CallError::TooLarge(input.len()));
        }
        let result = self.run(f, input);
        if matches!(result, Err(CallError::Deadline | CallError::Trap(_))) {
            self.spent = true;
        }
        result
    }

    /// Whether a call trapped or ran past its deadline ([`Instance::call`]).
    pub fn spent(&self) -> bool {
        self.spent
    }

    fn run(&mut self, f: TypedFunc<(i32, i32), i64>, input: &[u8]) -> Result<Vec<u8>, CallError> {
        self.store.set_epoch_deadline(if self.guard.is_some() {
            1
        } else {
            self.deadline_ticks
        });
        let len = input.len() as i32;
        let ptr = self.alloc.call(&mut self.store, len).map_err(trap)?;
        self.memory
            .write(&mut self.store, ptr as u32 as usize, input)
            .map_err(|_| CallError::Trap("octo_alloc returned memory out of bounds".into()))?;
        let packed = f.call(&mut self.store, (ptr, len)).map_err(trap)?;
        self.free.call(&mut self.store, (ptr, len)).map_err(trap)?;
        let out_ptr = (packed as u64 >> 32) as usize;
        let out_len = (packed as u64 & 0xffff_ffff) as usize;
        if out_len > self.io_bytes + 1 {
            self.free
                .call(&mut self.store, (out_ptr as i32, out_len as i32))
                .map_err(trap)?;
            return Err(CallError::TooLarge(out_len));
        }
        let mut out = vec![0u8; out_len];
        self.memory
            .read(&self.store, out_ptr, &mut out)
            .map_err(|_| CallError::Trap("the output is out of bounds".into()))?;
        self.free
            .call(&mut self.store, (out_ptr as i32, out_len as i32))
            .map_err(trap)?;
        match out.first() {
            Some(0) => {
                out.remove(0);
                Ok(out)
            }
            Some(1) => Err(CallError::Guest(
                String::from_utf8_lossy(&out[1..]).into_owned(),
            )),
            _ => Err(CallError::Trap("the output has no status byte".into())),
        }
    }

    /// The lines the module logged since the last read.
    pub fn take_logs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.store.data_mut().logs)
    }

    /// The instance's linear memory, in bytes.
    pub fn memory_bytes(&self) -> usize {
        self.memory.data_size(&self.store)
    }
}

fn trap(error: wasmtime::Error) -> CallError {
    match error.downcast_ref::<Trap>() {
        Some(Trap::Interrupt) => CallError::Deadline,
        Some(trap) => CallError::Trap(trap.to_string()),
        // A limit the store enforces: say which, without the wasm backtrace
        // wrapped around it.
        None => {
            let cause = error.root_cause().to_string();
            if cause.contains("growing memory") {
                CallError::Trap(format!("memory over its cap ({cause})"))
            } else {
                CallError::Trap(cause)
            }
        }
    }
}

/// Advances the engine's epoch every [`TICK`] until dropped.
struct Ticker {
    stop: Arc<AtomicBool>,
}

impl Ticker {
    fn start(engine: Engine) -> Ticker {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::Builder::new()
            .name("wasm-epoch".into())
            .spawn(move || {
                // Advance by the time that has passed, not by the sleeps: a
                // sleep can overrun, and a deadline is wall-clock time.
                let started = Instant::now();
                let mut ticks = 0u128;
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(TICK);
                    let due = started.elapsed().as_nanos() / TICK.as_nanos();
                    while ticks < due {
                        engine.increment_epoch();
                        ticks += 1;
                    }
                }
            })
            .expect("the wasm epoch thread");
        Ticker { stop }
    }
}

impl Drop for Ticker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
