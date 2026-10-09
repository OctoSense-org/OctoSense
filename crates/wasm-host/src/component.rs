//! An app's components (ADR 0014): a WebAssembly component built with plain
//! `cargo build --target wasm32-wasip2`, its exported functions called by
//! name with JSON, and a WASI 0.2 subset scoped to the app.
//!
//! # What a component reaches
//!
//! - Clocks and randomness.
//! - Its app's own storage folder, as the root of its filesystem, only when
//!   the app has a storage grant ([`Grants::storage_dir`]); nothing else of
//!   the host's filesystem.
//! - stdout and stderr, which become its log lines.
//!
//! No sockets, environment, arguments or stdin: a component that imports
//! `wasi:sockets`, `wasi:http` or anything outside WASI is refused when it
//! loads ([`LoadError::Import`]).
//!
//! # How a function is called
//!
//! [`ComponentInstance::call_json`] takes the arguments as JSON: an object
//! keyed by the function's parameter names, an array in their order, or,
//! for a function of one parameter, its value. WIT values map to JSON as in
//! [`to_val`] and [`from_val`]: records are objects, enums are strings,
//! options are `null` or the value, `list<u8>` is base64 text, and a
//! function's `result<T, E>` is its value or its error.
//!
//! Names face a script in snake_case: WIT's `to-html` is called as
//! `to_html`, and a record's `word-count` comes back as `word_count`, so a
//! script reads `r.data.word_count`. Either spelling is accepted on the
//! way in.
//!
//! Unlike a core module's, an instance keeps its state between calls: a
//! component may hold a parsed document, a cache or a model. A trap or a
//! deadline spends it, as with modules.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{json, Map, Value as Json};
use wasmtime::component::types::{ComponentItem, Type};
use wasmtime::component::{Component, Func, Linker, ResourceTable, Val};
use wasmtime::{Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::{trap, CallError, InvocationGuard, LoadError, Runtime, Ticker, LOG_LINE, LOG_LINES};

mod files;
mod host;
mod net;

pub use host::HostCalls;

/// The WASI packages a component may import. Every interface of these is
/// linked; what each can reach is set per instance ([`Grants`]).
const ALLOWED_PACKAGES: &[&str] = &[
    "wasi:cli/",
    "wasi:clocks/",
    "wasi:filesystem/",
    "wasi:http/",
    "wasi:io/",
    "wasi:random/",
    // The app's host services, as its script reaches them (`host`).
    "octosense:host/",
];

/// What a call that had a write refused logs, and adds to its error.
const REFUSED: &str = "a write was refused: the storage budget is used up";

/// How much one stream (stdout or stderr) buffers between reads.
const PIPE_BYTES: usize = 64 << 10;

/// Whether `bytes` is a component rather than a core module: the preamble's
/// version and layer fields differ (`0d 00 01 00` against `01 00 00 00`).
pub fn is_component(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes[..4] == *b"\0asm" && bytes[4..8] == [0x0d, 0x00, 0x01, 0x00]
}

/// What an instance of a component may reach beyond clocks and randomness.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grants {
    /// The app's storage folder, preopened as `/`. `None`: no filesystem.
    pub storage_dir: Option<PathBuf>,
    /// Open the storage folder read-only (its quota is used up).
    pub read_only: bool,
    /// The hosts its `wasi:http` requests may reach, by a script's rule
    /// (`host`, or `host:port`), over HTTPS. Empty: none.
    pub http_hosts: Vec<String>,
    /// Let those requests reach this device and its local network too, over
    /// plain HTTP as well (still only listed hosts): for tests and a
    /// developer's runs. The shell never sets it.
    pub http_local: bool,
}

/// One function an app may call, with its WIT-like signature.
#[derive(Clone, Debug, PartialEq)]
pub struct Export {
    /// The name a script calls it by, in snake_case: `name` for a world's
    /// own function, `interface.name` for one in an exported interface.
    pub name: String,
    /// The same as WIT spells it (`to-html`, `markdown.to-html`).
    pub wit_name: String,
    /// Parameter names and their types, as WIT writes them.
    pub params: Vec<(String, String)>,
    /// The result's type, as WIT writes it, if any.
    pub result: Option<String>,
}

/// A loaded component: checked, compiled (or taken from the cache).
#[derive(Clone)]
pub struct ComponentProgram {
    component: Component,
    exports: Vec<Export>,
    /// Exports a script cannot call (they take or return a resource), with
    /// the reason, so `wasm.functions` can say why they are missing.
    skipped: Vec<(String, String)>,
    from_cache: bool,
}

impl ComponentProgram {
    /// The functions an app may call, sorted by name.
    pub fn exports(&self) -> &[Export] {
        &self.exports
    }

    /// Exports left out, and why.
    pub fn skipped(&self) -> &[(String, String)] {
        &self.skipped
    }

    /// Whether the compiled code came from the cache.
    pub fn from_cache(&self) -> bool {
        self.from_cache
    }
}

struct State {
    wasi: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
    /// The running call's deadline and cancellation, read every epoch.
    guard: Option<Arc<InvocationGuard>>,
    /// What the running call may add to the storage folder.
    budget: files::Budget,
    http: wasmtime_wasi_http::WasiHttpCtx,
    /// The hosts its requests may reach, and what it was refused.
    hosts: net::Hosts,
    /// Its app's host services (`octosense:host`), when the embedder gives
    /// them.
    host_calls: host::Calls,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl wasmtime_wasi_http::WasiHttpView for State {
    fn http(&mut self) -> wasmtime_wasi_http::WasiHttpCtxView<'_> {
        wasmtime_wasi_http::WasiHttpCtxView {
            ctx: &mut self.http,
            table: &mut self.table,
            hooks: &mut self.hosts,
        }
    }
}

/// One app's instance of a [`ComponentProgram`]. It keeps its state between
/// calls until a trap or a deadline spends it.
pub struct ComponentInstance {
    store: Store<State>,
    funcs: HashMap<String, Func>,
    stdout: MemoryOutputPipe,
    stderr: MemoryOutputPipe,
    /// Bytes of stdout and stderr already turned into log lines.
    read: [usize; 2],
    logs: Vec<String>,
    deadline: Duration,
    io_bytes: usize,
    spent: bool,
    _ticker: Arc<Ticker>,
}

impl Runtime {
    /// Checks and compiles a component, or takes its compiled code from the
    /// cache. Its imports must all be in WASI's allowed packages.
    pub fn load_component(&self, bytes: &[u8]) -> Result<ComponentProgram, LoadError> {
        if bytes.len() > self.limits.module_bytes {
            return Err(LoadError::TooLarge(bytes.len()));
        }
        let cached = self.cache_path(bytes);
        let mut from_cache = false;
        let component = match cached.as_ref().and_then(|path| std::fs::read(path).ok()) {
            // SAFETY: the cache holds only what `Component::serialize` wrote
            // for this engine (the key includes its compatibility hash), in a
            // directory that belongs to the shell.
            Some(code) => match unsafe { Component::deserialize(&self.engine, &code) } {
                Ok(component) => {
                    from_cache = true;
                    component
                }
                Err(_) => self.compile_component(bytes, cached.as_ref())?,
            },
            None => self.compile_component(bytes, cached.as_ref())?,
        };
        let (exports, skipped) = check_component(&self.engine, &component)?;
        Ok(ComponentProgram {
            component,
            exports,
            skipped,
            from_cache,
        })
    }

    fn compile_component(
        &self,
        bytes: &[u8],
        cache: Option<&PathBuf>,
    ) -> Result<Component, LoadError> {
        let component = Component::new(&self.engine, bytes)
            .map_err(|e| LoadError::Invalid(format!("{e:#}")))?;
        // Check before caching: a refused component is never stored.
        check_component(&self.engine, &component)?;
        if let (Some(path), Ok(code)) = (cache, component.serialize()) {
            self.publish_cache(path, &code);
        }
        Ok(component)
    }

    /// An instance of `program` with `grants`. Its instantiation and every
    /// call share the runtime's deadline; `guard` adds a cancellation check
    /// and an overall deadline, as for modules.
    pub fn instantiate_component(
        &self,
        program: &ComponentProgram,
        grants: &Grants,
        guard: Option<(Instant, Box<dyn Fn() -> bool + Send + Sync>)>,
    ) -> Result<ComponentInstance, LoadError> {
        if guard
            .as_ref()
            .is_some_and(|(deadline, pending)| Instant::now() >= *deadline || !pending())
        {
            return Err(LoadError::Invalid(
                "invocation cancelled or past its deadline".into(),
            ));
        }
        let stdout = MemoryOutputPipe::new(PIPE_BYTES);
        let stderr = MemoryOutputPipe::new(PIPE_BYTES);
        let mut wasi = WasiCtxBuilder::new();
        wasi.stdout(stdout.clone())
            .stderr(stderr.clone())
            .allow_tcp(false)
            .allow_udp(false)
            .allow_ip_name_lookup(false)
            // Calls run on their app's own worker thread, synchronously: a
            // file operation blocks it directly rather than round-tripping
            // through a thread pool.
            .allow_blocking_current_thread(true);
        if let Some(dir) = &grants.storage_dir {
            wasi.preopened_dir(
                dir,
                "/",
                if grants.read_only {
                    FsPerms::ReadOnly
                } else {
                    FsPerms::ReadWrite
                },
            )
            .map_err(|e| LoadError::Invalid(format!("the app's storage: {e:#}")))?;
        }
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.limits.memory_bytes)
            .table_elements(self.limits.table_elements)
            .trap_on_grow_failure(true)
            .build();
        // Instantiation runs the component's start-up code: it gets the
        // same deadline as a call, and the caller's cancellation if any.
        let (deadline, pending) = match guard {
            Some((deadline, pending)) => (deadline, pending),
            None => (
                Instant::now() + self.limits.deadline,
                Box::new(|| true) as Box<dyn Fn() -> bool + Send + Sync>,
            ),
        };
        // A component that may reach the network waits for replies: its
        // calls get the longer deadline.
        let call_deadline = if grants.http_hosts.is_empty() {
            self.limits.deadline
        } else {
            self.limits.network_deadline.max(self.limits.deadline)
        };
        let guard = Arc::new(InvocationGuard {
            deadline: deadline.min(Instant::now() + call_deadline),
            pending,
        });
        let mut hosts = net::Hosts::new(grants.http_hosts.clone(), grants.http_local);
        hosts.deadline = Some(guard.deadline);
        let mut store = Store::new(
            &self.engine,
            State {
                wasi: wasi.build(),
                table: ResourceTable::new(),
                limits,
                guard: Some(guard),
                // Start-up code may read the folder, not grow it.
                budget: files::Budget::default(),
                http: wasmtime_wasi_http::WasiHttpCtx::new(),
                hosts,
                host_calls: None,
            },
        );
        store.data().budget.set(Some(0));
        store.limiter(|state| &mut state.limits);
        // Every epoch, the running call's guard decides: a call never runs
        // without one, so none outlives its deadline.
        store.epoch_deadline_callback(|ctx| {
            Ok(match &ctx.data().guard {
                Some(guard) if guard.live() => wasmtime::UpdateDeadline::Continue(1),
                _ => wasmtime::UpdateDeadline::Interrupt,
            })
        });
        store.set_epoch_deadline(1);
        let mut linker = Linker::new(&self.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .and_then(|()| wasmtime_wasi_http::p2::add_only_http_to_linker_sync(&mut linker))
            .and_then(|()| files::link(&mut linker))
            .and_then(|()| host::link(&mut linker))
            .map_err(|e| LoadError::Invalid(format!("{e:#}")))?;
        let instance = linker
            .instantiate(&mut store, &program.component)
            .map_err(|e| LoadError::Invalid(format!("{e:#}")))?;
        store.data_mut().guard = None;
        store.data().budget.set(None);
        let mut funcs = HashMap::new();
        for export in &program.exports {
            let func = match export.wit_name.split_once('.') {
                None => instance.get_func(&mut store, &export.wit_name),
                Some((iface, name)) => {
                    let full = interface_export(&program.component, &self.engine, iface);
                    full.and_then(|full| instance.get_export_index(&mut store, None, &full))
                        .and_then(|idx| instance.get_export_index(&mut store, Some(&idx), name))
                        .and_then(|idx| instance.get_func(&mut store, idx))
                }
            };
            let func = func.ok_or_else(|| LoadError::Export(export.name.clone()))?;
            funcs.insert(export.name.clone(), func);
        }
        Ok(ComponentInstance {
            store,
            funcs,
            stdout,
            stderr,
            read: [0, 0],
            logs: Vec::new(),
            deadline: call_deadline,
            io_bytes: self.limits.io_bytes,
            spent: false,
            _ticker: self.ticker.clone(),
        })
    }
}

/// The full name (`pkg:ns/iface@version`) of the exported interface whose
/// short name is `short`.
fn interface_export(
    component: &Component,
    engine: &wasmtime::Engine,
    short: &str,
) -> Option<String> {
    component
        .component_type()
        .exports(engine)
        .find(|(name, ext)| {
            matches!(ext.ty, ComponentItem::ComponentInstance(_)) && short_name(name) == short
        })
        .map(|(name, _)| name.to_string())
}

/// WIT's kebab-case as a script spells it: `word-count` → `word_count`.
pub fn snake(name: &str) -> String {
    name.replace('-', "_")
}

/// `my:pkg/markdown@0.1.0` → `markdown`.
fn short_name(full: &str) -> &str {
    let tail = full.rsplit('/').next().unwrap_or(full);
    tail.split('@').next().unwrap_or(tail)
}

/// The imports a component may have, and the functions a script may call.
fn check_component(
    engine: &wasmtime::Engine,
    component: &Component,
) -> Result<(Vec<Export>, Vec<(String, String)>), LoadError> {
    let ty = component.component_type();
    for (name, _) in ty.imports(engine) {
        if !ALLOWED_PACKAGES.iter().any(|p| name.starts_with(p)) {
            return Err(LoadError::Import(name.to_string()));
        }
    }
    let mut exports = Vec::new();
    let mut skipped = Vec::new();
    for (name, ext) in ty.exports(engine) {
        match ext.ty {
            ComponentItem::ComponentFunc(func) => {
                add_export(name.to_string(), &func, &mut exports, &mut skipped);
            }
            ComponentItem::ComponentInstance(instance) => {
                let iface = short_name(name);
                for (fname, fext) in instance.exports(engine) {
                    if let ComponentItem::ComponentFunc(func) = fext.ty {
                        add_export(
                            format!("{iface}.{fname}"),
                            &func,
                            &mut exports,
                            &mut skipped,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    exports.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((exports, skipped))
}

fn add_export(
    name: String,
    func: &wasmtime::component::types::ComponentFunc,
    exports: &mut Vec<Export>,
    skipped: &mut Vec<(String, String)>,
) {
    let params: Vec<(String, Type)> = func.params().map(|(n, t)| (n.to_string(), t)).collect();
    let results: Vec<Type> = func.results().collect();
    if let Some(why) = params
        .iter()
        .map(|(_, t)| t)
        .chain(results.iter())
        .find_map(unsupported)
    {
        skipped.push((name, why));
        return;
    }
    exports.push(Export {
        name: snake(&name),
        wit_name: name,
        params: params.iter().map(|(n, t)| (snake(n), wit(t))).collect(),
        result: match results.len() {
            0 => None,
            1 => Some(wit(&results[0])),
            _ => Some(format!(
                "tuple<{}>",
                results.iter().map(wit).collect::<Vec<_>>().join(", ")
            )),
        },
    });
}

/// Why a script cannot pass or receive a value of this type, if it cannot.
fn unsupported(ty: &Type) -> Option<String> {
    match ty {
        Type::Own(_) | Type::Borrow(_) => Some("it takes or returns a resource".into()),
        Type::List(list) => unsupported(&list.ty()),
        Type::Option(opt) => unsupported(&opt.ty()),
        Type::Result(res) => res
            .ok()
            .as_ref()
            .and_then(unsupported)
            .or_else(|| res.err().as_ref().and_then(unsupported)),
        Type::Record(record) => record.fields().find_map(|f| unsupported(&f.ty)),
        Type::Tuple(tuple) => tuple.types().find_map(|t| unsupported(&t)),
        Type::Variant(variant) => variant
            .cases()
            .find_map(|c| c.ty.as_ref().and_then(unsupported)),
        Type::Bool
        | Type::S8
        | Type::U8
        | Type::S16
        | Type::U16
        | Type::S32
        | Type::U32
        | Type::S64
        | Type::U64
        | Type::Float32
        | Type::Float64
        | Type::Char
        | Type::String
        | Type::Enum(_)
        | Type::Flags(_) => None,
        #[allow(unreachable_patterns)]
        _ => Some("its type is not one a script can use".into()),
    }
}

/// A type as WIT writes it (records, variants and enums by their shape).
fn wit(ty: &Type) -> String {
    match ty {
        Type::Bool => "bool".into(),
        Type::S8 => "s8".into(),
        Type::U8 => "u8".into(),
        Type::S16 => "s16".into(),
        Type::U16 => "u16".into(),
        Type::S32 => "s32".into(),
        Type::U32 => "u32".into(),
        Type::S64 => "s64".into(),
        Type::U64 => "u64".into(),
        Type::Float32 => "f32".into(),
        Type::Float64 => "f64".into(),
        Type::Char => "char".into(),
        Type::String => "string".into(),
        Type::List(list) => format!("list<{}>", wit(&list.ty())),
        Type::Option(opt) => format!("option<{}>", wit(&opt.ty())),
        Type::Result(res) => match (res.ok(), res.err()) {
            (Some(ok), Some(err)) => format!("result<{}, {}>", wit(&ok), wit(&err)),
            (Some(ok), None) => format!("result<{}>", wit(&ok)),
            (None, Some(err)) => format!("result<_, {}>", wit(&err)),
            (None, None) => "result".into(),
        },
        Type::Tuple(tuple) => format!(
            "tuple<{}>",
            tuple
                .types()
                .map(|t| wit(&t))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Record(record) => format!(
            "record {{ {} }}",
            record
                .fields()
                .map(|f| format!("{}: {}", f.name, wit(&f.ty)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Variant(variant) => format!(
            "variant {{ {} }}",
            variant
                .cases()
                .map(|c| match &c.ty {
                    Some(t) => format!("{}({})", c.name, wit(t)),
                    None => c.name.to_string(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Enum(e) => format!("enum {{ {} }}", e.names().collect::<Vec<_>>().join(", ")),
        Type::Flags(f) => format!("flags {{ {} }}", f.names().collect::<Vec<_>>().join(", ")),
        _ => "resource".into(),
    }
}

impl ComponentInstance {
    /// The functions this instance has, sorted.
    pub fn functions(&self) -> Vec<String> {
        let mut names: Vec<String> = self.funcs.keys().cloned().collect();
        names.sort();
        names
    }

    /// Whether a call trapped or ran past its deadline.
    pub fn spent(&self) -> bool {
        self.spent
    }

    /// What the next calls may add to the storage folder (`None`, the
    /// default: no ceiling). A write past it fails in the guest as a full
    /// disk; overwriting, truncating and deleting give bytes back.
    pub fn set_storage_budget(&mut self, left: Option<u64>) {
        self.store.data().budget.set(left);
    }

    /// What is left of the storage budget.
    pub fn storage_budget(&self) -> Option<u64> {
        self.store.data().budget.left()
    }

    /// The app's host services its `octosense:host` calls reach (`None`,
    /// the default: none).
    pub fn set_host_calls(&mut self, calls: Option<Arc<dyn HostCalls>>) {
        self.store.data_mut().host_calls = calls;
    }

    /// Calls `name` with JSON arguments; returns its JSON result. A
    /// `result<T, E>` return is `T`, or [`CallError::Guest`] with `E`.
    pub fn call_json(&mut self, name: &str, args: &Json) -> Result<Json, CallError> {
        self.call_json_guarded(name, args, Instant::now() + self.deadline, || true)
    }

    /// [`ComponentInstance::call_json`], ended at `deadline` (no later than
    /// the runtime's) or as soon as `pending` answers false.
    pub fn call_json_guarded(
        &mut self,
        name: &str,
        args: &Json,
        deadline: Instant,
        pending: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Json, CallError> {
        if self.spent {
            return Err(CallError::Spent);
        }
        let guard = Arc::new(InvocationGuard {
            deadline: deadline.min(Instant::now() + self.deadline),
            pending: Box::new(pending),
        });
        if !guard.live() {
            return Err(CallError::Deadline);
        }
        let func = *self
            .funcs
            .get(name)
            .or_else(|| self.funcs.get(&snake(name)))
            .ok_or_else(|| CallError::NoSuchFunction(name.into()))?;
        let size = serde_json::to_vec(args).map(|v| v.len()).unwrap_or(0);
        if size > self.io_bytes {
            return Err(CallError::TooLarge(size));
        }
        let fty = func.ty(&self.store);
        let params: Vec<(String, Type)> = fty.params().map(|(n, t)| (n.to_string(), t)).collect();
        let args = arguments(&params, args).map_err(CallError::Guest)?;
        let results_ty: Vec<Type> = fty.results().collect();
        let mut results = vec![Val::Bool(false); results_ty.len()];
        self.store.data_mut().hosts.deadline = Some(guard.deadline);
        self.store.data_mut().guard = Some(guard);
        self.store.set_epoch_deadline(1);
        let outcome = func
            .call(&mut self.store, &args, &mut results)
            .map_err(trap);
        self.store.data_mut().guard = None;
        self.collect_logs();
        for (host, why) in self.store.data_mut().hosts.take_refused() {
            self.logs
                .push(format!("a request to {host} was refused: {why}"));
        }
        // A refused write reaches the guest as a full disk or, through a
        // stream, a bare I/O error: say what it was.
        let refused = self.store.data().budget.take_refused();
        if refused {
            self.logs.push(REFUSED.into());
        }
        if let Err(error) = outcome {
            if matches!(error, CallError::Deadline | CallError::Trap(_)) {
                self.spent = true;
            }
            return Err(error);
        }
        let value = match (results_ty.as_slice(), results.as_slice()) {
            ([], _) => Json::Null,
            ([Type::Result(res)], [Val::Result(Ok(ok))]) => ok
                .as_deref()
                .zip(res.ok())
                .map(|(v, t)| from_val(&t, v))
                .unwrap_or(Json::Null),
            ([Type::Result(res)], [Val::Result(Err(err))]) => {
                let why = match err.as_deref().zip(res.err()).map(|(v, t)| from_val(&t, v)) {
                    Some(Json::String(text)) => text,
                    Some(other) => other.to_string(),
                    None => "the function failed".into(),
                };
                return Err(CallError::Guest(if refused {
                    format!("{why}; {REFUSED}")
                } else {
                    why
                }));
            }
            ([ty], [one]) => from_val(ty, one),
            (types, many) => Json::Array(
                types
                    .iter()
                    .zip(many)
                    .map(|(t, v)| from_val(t, v))
                    .collect(),
            ),
        };
        let size = serde_json::to_vec(&value).map(|v| v.len()).unwrap_or(0);
        if size > self.io_bytes {
            return Err(CallError::TooLarge(size));
        }
        Ok(value)
    }

    /// The lines the component wrote to stdout or stderr since the last read.
    pub fn take_logs(&mut self) -> Vec<String> {
        self.collect_logs();
        std::mem::take(&mut self.logs)
    }

    fn collect_logs(&mut self) {
        for (i, pipe) in [&self.stdout, &self.stderr].into_iter().enumerate() {
            let bytes = pipe.contents();
            if bytes.len() <= self.read[i] {
                continue;
            }
            let fresh = String::from_utf8_lossy(&bytes[self.read[i]..]).into_owned();
            self.read[i] = bytes.len();
            for line in fresh.lines().filter(|l| !l.is_empty()) {
                if self.logs.len() < LOG_LINES {
                    let mut line = line.to_string();
                    line.truncate(LOG_LINE);
                    self.logs.push(line);
                }
            }
        }
    }
}

/// The call's arguments from JSON. One parameter: `{name: value}` or the
/// value itself. Several: an object keyed by their names, or an array in
/// their order. None: `null`, `{}` or `[]`.
fn arguments(params: &[(String, Type)], args: &Json) -> Result<Vec<Val>, String> {
    let named = |map: &Map<String, Json>| -> Result<Vec<Val>, String> {
        params
            .iter()
            .map(|(n, t)| {
                let value = field(map, n).unwrap_or(&Json::Null);
                to_val(t, value).map_err(|e| format!("{}: {e}", snake(n)))
            })
            .collect()
    };
    let has_all = |map: &Map<String, Json>| {
        map.len() == params.len() && params.iter().all(|(n, _)| field(map, n).is_some())
    };
    match (params, args) {
        ([], Json::Null) => Ok(Vec::new()),
        ([], Json::Object(map)) if map.is_empty() => Ok(Vec::new()),
        ([], Json::Array(items)) if items.is_empty() => Ok(Vec::new()),
        ([_], Json::Object(map)) if has_all(map) => named(map),
        // The value itself or, when it is not one, a one-item array of it.
        ([(name, ty)], value) => match (to_val(ty, value), value) {
            (Ok(one), _) => Ok(vec![one]),
            (Err(_), Json::Array(items)) if items.len() == 1 => Ok(vec![
                to_val(ty, &items[0]).map_err(|e| format!("{}: {e}", snake(name)))?
            ]),
            (Err(e), _) => Err(format!("{}: {e}", snake(name))),
        },
        (_, Json::Object(map)) if has_all(map) => named(map),
        (_, Json::Array(items)) if items.len() == params.len() => params
            .iter()
            .zip(items)
            .map(|((n, t), v)| to_val(t, v).map_err(|e| format!("{}: {e}", snake(n))))
            .collect(),
        _ => Err(format!(
            "takes {}: pass an object with those names",
            params
                .iter()
                .map(|(n, _)| snake(n))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The value under a WIT name, spelled either way.
fn field<'a>(map: &'a Map<String, Json>, wit_name: &str) -> Option<&'a Json> {
    map.get(wit_name).or_else(|| map.get(&snake(wit_name)))
}

/// JSON to a WIT value of type `ty`.
pub fn to_val(ty: &Type, v: &Json) -> Result<Val, String> {
    fn int<T: TryFrom<i64> + TryFrom<u64>>(v: &Json, what: &str) -> Result<T, String> {
        let out = match (v.as_i64(), v.as_u64()) {
            (_, Some(u)) => T::try_from(u).ok(),
            (Some(i), _) => T::try_from(i).ok(),
            _ => None,
        };
        out.ok_or_else(|| format!("expected {what}, got {v}"))
    }
    Ok(match ty {
        Type::Bool => Val::Bool(
            v.as_bool()
                .ok_or_else(|| format!("expected bool, got {v}"))?,
        ),
        Type::S8 => Val::S8(int(v, "s8")?),
        Type::U8 => Val::U8(int(v, "u8")?),
        Type::S16 => Val::S16(int(v, "s16")?),
        Type::U16 => Val::U16(int(v, "u16")?),
        Type::S32 => Val::S32(int(v, "s32")?),
        Type::U32 => Val::U32(int(v, "u32")?),
        Type::S64 => Val::S64(int(v, "s64")?),
        Type::U64 => Val::U64(int(v, "u64")?),
        Type::Float32 => {
            Val::Float32(v.as_f64().ok_or_else(|| format!("expected f32, got {v}"))? as f32)
        }
        Type::Float64 => Val::Float64(v.as_f64().ok_or_else(|| format!("expected f64, got {v}"))?),
        Type::Char => {
            let s = v
                .as_str()
                .ok_or_else(|| format!("expected char, got {v}"))?;
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Val::Char(c),
                _ => return Err(format!("expected one character, got {v}")),
            }
        }
        Type::String => Val::String(
            v.as_str()
                .ok_or_else(|| format!("expected string, got {v}"))?
                .to_string(),
        ),
        Type::List(list) => match (list.ty(), v) {
            // Bytes travel as base64 text; an array of numbers also works.
            (Type::U8, Json::String(text)) => Val::List(
                base64::engine::general_purpose::STANDARD
                    .decode(text)
                    .map_err(|e| format!("expected base64 bytes: {e}"))?
                    .into_iter()
                    .map(Val::U8)
                    .collect(),
            ),
            (item, Json::Array(items)) => Val::List(
                items
                    .iter()
                    .map(|i| to_val(&item, i))
                    .collect::<Result<_, _>>()?,
            ),
            _ => return Err(format!("expected a list, got {v}")),
        },
        Type::Record(record) => {
            let map = v
                .as_object()
                .ok_or_else(|| format!("expected an object, got {v}"))?;
            Val::Record(
                record
                    .fields()
                    .map(|f| {
                        let value = field(map, f.name).unwrap_or(&Json::Null);
                        to_val(&f.ty, value)
                            .map(|val| (f.name.to_string(), val))
                            .map_err(|e| format!("{}: {e}", f.name))
                    })
                    .collect::<Result<_, _>>()?,
            )
        }
        Type::Tuple(tuple) => {
            let items = v
                .as_array()
                .ok_or_else(|| format!("expected an array, got {v}"))?;
            let types: Vec<Type> = tuple.types().collect();
            if items.len() != types.len() {
                return Err(format!(
                    "expected {} items, got {}",
                    types.len(),
                    items.len()
                ));
            }
            Val::Tuple(
                types
                    .iter()
                    .zip(items)
                    .map(|(t, i)| to_val(t, i))
                    .collect::<Result<_, _>>()?,
            )
        }
        Type::Variant(variant) => {
            let (case, payload) = match v {
                Json::String(name) => (name.as_str(), None),
                Json::Object(map) if map.len() == 1 => {
                    let (k, val) = map.iter().next().expect("one entry");
                    (k.as_str(), Some(val))
                }
                _ => return Err(format!("expected a case name or {{case: value}}, got {v}")),
            };
            let found = variant
                .cases()
                .find(|c| c.name == case || snake(c.name) == case)
                .ok_or_else(|| format!("no case {case}"))?;
            let payload = match (&found.ty, payload) {
                (Some(t), Some(p)) => Some(Box::new(to_val(t, p)?)),
                (Some(_), None) => return Err(format!("case {case} takes a value")),
                (None, _) => None,
            };
            Val::Variant(found.name.to_string(), payload)
        }
        Type::Enum(e) => {
            let name = v
                .as_str()
                .ok_or_else(|| format!("expected a name, got {v}"))?;
            let wit_name = e
                .names()
                .find(|n| *n == name || snake(n) == name)
                .ok_or_else(|| format!("no case {name}"))?;
            Val::Enum(wit_name.to_string())
        }
        Type::Option(opt) => match v {
            Json::Null => Val::Option(None),
            other => Val::Option(Some(Box::new(to_val(&opt.ty(), other)?))),
        },
        Type::Result(res) => {
            let map = v
                .as_object()
                .filter(|m| m.len() == 1)
                .ok_or_else(|| format!("expected {{ok: …}} or {{err: …}}, got {v}"))?;
            let (k, val) = map.iter().next().expect("one entry");
            let side = match k.as_str() {
                "ok" => res.ok(),
                "err" => res.err(),
                _ => return Err(format!("expected ok or err, got {k}")),
            };
            let payload = match side {
                Some(t) => Some(Box::new(to_val(&t, val)?)),
                None => None,
            };
            Val::Result(if k == "ok" { Ok(payload) } else { Err(payload) })
        }
        Type::Flags(f) => {
            let items = v
                .as_array()
                .ok_or_else(|| format!("expected a list of names, got {v}"))?;
            let mut out = Vec::new();
            for item in items {
                let name = item
                    .as_str()
                    .ok_or_else(|| format!("expected a name, got {item}"))?;
                let wit_name = f
                    .names()
                    .find(|n| *n == name || snake(n) == name)
                    .ok_or_else(|| format!("no flag {name}"))?;
                out.push(wit_name.to_string());
            }
            Val::Flags(out)
        }
        _ => return Err("resources cannot be passed from a script".into()),
    })
}

/// A WIT value of type `ty` as JSON.
pub fn from_val(ty: &Type, v: &Val) -> Json {
    match (ty, v) {
        (_, Val::Bool(b)) => json!(b),
        (_, Val::S8(n)) => json!(n),
        (_, Val::U8(n)) => json!(n),
        (_, Val::S16(n)) => json!(n),
        (_, Val::U16(n)) => json!(n),
        (_, Val::S32(n)) => json!(n),
        (_, Val::U32(n)) => json!(n),
        (_, Val::S64(n)) => json!(n),
        (_, Val::U64(n)) => json!(n),
        (_, Val::Float32(n)) => json!(n),
        (_, Val::Float64(n)) => json!(n),
        (_, Val::Char(c)) => json!(c.to_string()),
        (_, Val::String(s)) => json!(s),
        (Type::List(list), Val::List(items)) => match list.ty() {
            Type::U8 => {
                let bytes: Vec<u8> = items
                    .iter()
                    .map(|i| match i {
                        Val::U8(b) => *b,
                        _ => 0,
                    })
                    .collect();
                json!(base64::engine::general_purpose::STANDARD.encode(bytes))
            }
            item => Json::Array(items.iter().map(|i| from_val(&item, i)).collect()),
        },
        (Type::Record(record), Val::Record(fields)) => {
            let mut map = Map::new();
            for (f, (k, val)) in record.fields().zip(fields) {
                map.insert(snake(k), from_val(&f.ty, val));
            }
            Json::Object(map)
        }
        (Type::Tuple(tuple), Val::Tuple(items)) => Json::Array(
            tuple
                .types()
                .zip(items)
                .map(|(t, i)| from_val(&t, i))
                .collect(),
        ),
        (Type::Variant(variant), Val::Variant(case, payload)) => {
            let ty = variant.cases().find(|c| c.name == case).and_then(|c| c.ty);
            match (ty, payload) {
                (Some(t), Some(p)) => json!({ snake(case): from_val(&t, p) }),
                _ => json!(snake(case)),
            }
        }
        (_, Val::Enum(name)) => json!(snake(name)),
        (Type::Option(opt), Val::Option(o)) => o
            .as_deref()
            .map(|v| from_val(&opt.ty(), v))
            .unwrap_or(Json::Null),
        (Type::Result(res), Val::Result(r)) => match r {
            Ok(v) => json!({"ok": v.as_deref().zip(res.ok()).map(|(v, t)| from_val(&t, v))}),
            Err(e) => json!({"err": e.as_deref().zip(res.err()).map(|(v, t)| from_val(&t, v))}),
        },
        (_, Val::Flags(names)) => json!(names.iter().map(|n| snake(n)).collect::<Vec<_>>()),
        _ => Json::Null,
    }
}
