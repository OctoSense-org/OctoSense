//! octosense-kernel: the shell's octos kernel.
//!
//! The octos agent kernel is a shell service. A shell (OctoSense-ROM Home,
//! OctoSense-Desktop) [`configure`]s it once at startup; every consumer —
//! the AppCard assistant today, Rinx's native mini-app host next — calls
//! [`connect`] and gets its own [`Connection`] to the ONE kernel of the
//! process. The AI providers app's `llm` host service writes the kernel's
//! profile under [`core_dir`] and calls [`restart`] after a change.
//!
//! - **Lazy, single instance.** The first `connect()` starts the kernel; the
//!   next ones share it. octos holds a single-writer lock on its data dir,
//!   so two kernels on one core dir could not coexist anyway.
//! - **Shared by frames.** A connection carries UI Protocol (JSON-RPC)
//!   frames, one per `send`/`recv`, exactly as `octos serve --stdio` speaks
//!   them. Each consumer uses its own request ids and sees the replies to its
//!   own requests and the notifications of the sessions it opened (see
//!   `router`).
//! - **Restart.** [`restart`] stops a running kernel (a no-op when none
//!   runs); each connection's `recv` then returns
//!   [`CloseReason::Restarted`], and the consumer reconnects, which starts a
//!   fresh kernel that reads the new profile.
//! - **Idle stop.** When the last connection is dropped the kernel stops, as
//!   AppCard's own `kill_on_drop` child did.
//! - **Shutdown.** [`shutdown`] stops it and waits.
//!
//! Where it runs: see [`launch`]. The kernel and this crate's frame pump run
//! on a runtime of their own (8 MiB worker stacks: the embedded core's
//! dispatcher needs them), so consumers may use any runtime or none.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tokio::sync::{mpsc, watch};

pub mod dirs;
mod kernel;
pub mod launch;
mod router;

pub use dirs::{kernel_home, profile_path, resolve_core_dir};
pub use launch::{Launch, Unavailable};

use kernel::{Ctl, Inbound};

/// The version of the embedded core (OpenHarmony).
#[cfg(target_env = "ohos")]
pub const EMBEDDED_VERSION: &str = octos_cli::embedded::VERSION;

/// Where kernel and core diagnostics go (the kernel's stderr, starts and
/// stops). Default: `log::info!`.
pub type LogSink = Arc<dyn Fn(&str) + Send + Sync>;

/// What a shell tells the core. Every field is optional.
#[derive(Clone, Default)]
pub struct Options {
    core_dir: Option<PathBuf>,
    app_data_dir: Option<PathBuf>,
    program: Option<PathBuf>,
    env: Vec<(String, String)>,
    log: Option<LogSink>,
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field("core_dir", &self.core_dir)
            .field("app_data_dir", &self.app_data_dir)
            .field("program", &self.program)
            .field("env", &self.env.iter().map(|(k, _)| k).collect::<Vec<_>>())
            .finish()
    }
}

impl Options {
    /// The kernel's core dir (octos data dir, `<core_dir>/profiles/_main.json`).
    /// Default: see [`resolve_core_dir`].
    pub fn core_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.core_dir = Some(dir.into());
        self
    }
    /// The app's data dir (`cx.get_data_dir()`): on a phone the core dir is
    /// `<data dir>/octos-home/.octos`.
    pub fn app_data_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.app_data_dir = Some(dir.into());
        self
    }
    /// The kernel binary (desktop; on Android it overrides the bundled
    /// `liboctos.so`). Default: `$OCTOS_APP_CORE_BIN`.
    pub fn program(mut self, program: impl Into<PathBuf>) -> Self {
        self.program = Some(program.into());
        self
    }
    /// Extra environment for a child kernel.
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
    /// Where diagnostics go.
    pub fn log(mut self, sink: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.log = Some(Arc::new(sink));
        self
    }
}

/// Why a connection ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseReason {
    /// [`restart`] stopped the kernel: connect again for the new one.
    Restarted,
    /// [`shutdown`], or the core went away.
    Shutdown,
    /// The kernel ended on its own (crashed, lost its pipe, refused to start
    /// on a locked data dir, …); the text says what it said last.
    Exited(String),
    /// The kernel could not start.
    Failed(String),
}

impl std::fmt::Display for CloseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Restarted => f.write_str("the octos kernel restarted"),
            Self::Shutdown => f.write_str("the octos kernel was shut down"),
            Self::Exited(why) => write!(f, "the octos kernel stopped: {why}"),
            Self::Failed(why) => write!(f, "the octos kernel could not start: {why}"),
        }
    }
}

impl std::error::Error for CloseReason {}

/// What [`status`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    /// A kernel generation is running (or starting).
    pub running: bool,
    /// The number of the current (or last) generation; 0 before the first.
    pub generation: u64,
    /// Open connections to the current generation.
    pub connections: usize,
}

struct Generation {
    id: u64,
    ctl: mpsc::UnboundedSender<Ctl>,
    connections: usize,
}

struct State {
    options: Options,
    current: Option<Generation>,
    /// Set once the previous generation released its kernel.
    last_done: Option<watch::Receiver<bool>>,
    generations: u64,
    next_conn: u64,
}

struct Inner {
    state: Mutex<State>,
    runtime: OnceLock<tokio::runtime::Runtime>,
}

impl Inner {
    fn runtime(&self) -> &tokio::runtime::Runtime {
        self.runtime.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                // The embedded core's AppUI dispatcher overflows Tokio's
                // default 2 MiB worker stack (octos_cli::embedded docs).
                .thread_stack_size(8 * 1024 * 1024)
                .thread_name("octos-core")
                .enable_all()
                .build()
                .expect("octos-core: tokio runtime")
        })
    }

    fn log(options: &Options) -> LogSink {
        options.log.clone().unwrap_or_else(|| Arc::new(|line: &str| log::info!("{line}")))
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // A core may be dropped from inside some other runtime (a test's, a
        // consumer's): never block there.
        if let Some(rt) = self.runtime.take() {
            rt.shutdown_background();
        }
    }
}

/// One kernel service. The process-wide one is behind the free functions
/// ([`configure`], [`connect`], …); tests make their own.
#[derive(Clone)]
pub struct Core(Arc<Inner>);

impl Default for Core {
    fn default() -> Self {
        Self::new(Options::default())
    }
}

impl Core {
    pub fn new(options: Options) -> Self {
        Core(Arc::new(Inner {
            state: Mutex::new(State {
                options,
                current: None,
                last_done: None,
                generations: 0,
                next_conn: 0,
            }),
            runtime: OnceLock::new(),
        }))
    }

    /// Replace the options. A running kernel keeps its launch until it
    /// restarts; the core dir answers the new value at once.
    pub fn configure(&self, options: Options) {
        self.0.state.lock().unwrap().options = options;
    }

    /// The kernel's core dir (see [`resolve_core_dir`]).
    pub fn core_dir(&self) -> Option<PathBuf> {
        let st = self.0.state.lock().unwrap();
        Self::core_dir_of(&st.options)
    }

    fn core_dir_of(options: &Options) -> Option<PathBuf> {
        resolve_core_dir(options.core_dir.as_deref(), options.app_data_dir.as_deref())
    }

    /// How the kernel would start, or why it cannot.
    pub fn launch(&self) -> Result<Launch, Unavailable> {
        let st = self.0.state.lock().unwrap();
        Self::launch_of(&st.options)
    }

    fn launch_of(options: &Options) -> Result<Launch, Unavailable> {
        let core_dir = Self::core_dir_of(options);
        launch::resolve(&launch::Inputs {
            core_dir: core_dir.as_deref(),
            program: options.program.as_deref(),
            env: &options.env,
        })
    }

    /// Connect to the kernel, starting it if none runs. Fails only when no
    /// kernel can exist here; a kernel that then fails to start closes the
    /// connection with [`CloseReason::Failed`].
    pub fn connect(&self) -> Result<Connection, Unavailable> {
        let mut st = self.0.state.lock().unwrap();
        let conn = st.next_conn;
        st.next_conn += 1;
        let (tx, rx) = mpsc::unbounded_channel();
        // Join the running generation if it still takes consumers.
        if let Some(current) = st.current.as_mut() {
            if current.ctl.send(Ctl::Attach(conn, tx.clone())).is_ok() {
                current.connections += 1;
                return Ok(Connection {
                    core: self.clone(),
                    id: conn,
                    generation: current.id,
                    ctl: current.ctl.clone(),
                    inbound: rx,
                    closed: None,
                });
            }
            st.current = None;
        }
        let launch = Self::launch_of(&st.options)?;
        let core_dir = Self::core_dir_of(&st.options).ok_or(Unavailable::NoCoreDir)?;
        let log = Inner::log(&st.options);
        st.generations += 1;
        let id = st.generations;
        let (ctl, ctl_rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = watch::channel(false);
        let previous = st.last_done.replace(done_rx);
        ctl.send(Ctl::Attach(conn, tx)).expect("fresh channel");
        st.current = Some(Generation { id, ctl: ctl.clone(), connections: 1 });
        drop(st);

        let weak = Arc::downgrade(&self.0);
        self.0.runtime().spawn(async move {
            // Wait until the previous kernel let go of the data dir, then
            // make the directories (and the phone's kernel config).
            if let Some(mut previous) = previous {
                let _ = previous.wait_for(|stopped| *stopped).await;
            }
            launch::prepare(&launch, &core_dir);
            let ended = move || {
                if let Some(inner) = weak.upgrade() {
                    let mut st = inner.state.lock().unwrap();
                    if st.current.as_ref().is_some_and(|g| g.id == id) {
                        st.current = None;
                    }
                }
            };
            kernel::supervise(id, launch, ctl_rx, done_tx, log, ended).await;
        });
        Ok(Connection { core: self.clone(), id: conn, generation: id, ctl, inbound: rx, closed: None })
    }

    /// Restart the kernel if one runs: its connections close with
    /// [`CloseReason::Restarted`] and the next `connect()` starts a fresh
    /// kernel (after this one let go of its data dir). Returns whether one
    /// was running. Callable from any thread.
    pub fn restart(&self) -> bool {
        self.stop_current(CloseReason::Restarted)
    }

    /// Stop the kernel (if any) and wait up to `timeout` for it to exit.
    pub fn shutdown_within(&self, timeout: Duration) -> bool {
        let running = self.stop_current(CloseReason::Shutdown);
        let done = self.0.state.lock().unwrap().last_done.clone();
        if let (Some(mut done), Some(rt)) = (done, self.0.runtime.get()) {
            let wait = async move {
                let _ = tokio::time::timeout(timeout, done.wait_for(|d| *d)).await;
            };
            // Blocking here is fine off the core's own runtime threads.
            let (tx, rx) = std::sync::mpsc::channel();
            rt.spawn(async move {
                wait.await;
                let _ = tx.send(());
            });
            let _ = rx.recv_timeout(timeout + Duration::from_millis(100));
        }
        running
    }

    fn stop_current(&self, reason: CloseReason) -> bool {
        let current = self.0.state.lock().unwrap().current.take();
        match current {
            Some(generation) => {
                log::info!("octos-core: {reason} (kernel {})", generation.id);
                let _ = generation.ctl.send(Ctl::Stop(reason));
                true
            }
            None => false,
        }
    }

    pub fn status(&self) -> Status {
        let st = self.0.state.lock().unwrap();
        Status {
            running: st.current.is_some(),
            generation: st.generations,
            connections: st.current.as_ref().map_or(0, |g| g.connections),
        }
    }

    /// A connection left: when it was the last of the running generation,
    /// the kernel stops.
    fn detach(&self, generation: u64, conn: u64) {
        let mut st = self.0.state.lock().unwrap();
        let Some(current) = st.current.as_mut().filter(|g| g.id == generation) else {
            return;
        };
        let _ = current.ctl.send(Ctl::Detach(conn));
        current.connections = current.connections.saturating_sub(1);
        if current.connections == 0 {
            let idle = st.current.take().expect("checked above");
            let _ = idle.ctl.send(Ctl::Stop(CloseReason::Shutdown));
        }
    }
}

/// One consumer's connection to the kernel: send frames, receive the replies
/// to them and its sessions' notifications. Dropping it detaches.
pub struct Connection {
    core: Core,
    id: u64,
    generation: u64,
    ctl: mpsc::UnboundedSender<Ctl>,
    inbound: mpsc::UnboundedReceiver<Inbound>,
    closed: Option<CloseReason>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").field("id", &self.id).field("generation", &self.generation).finish()
    }
}

impl Connection {
    /// Send one JSON-RPC frame (no trailing newline needed). Never blocks.
    pub fn send(&self, frame: impl Into<String>) -> Result<(), CloseReason> {
        if let Some(reason) = &self.closed {
            return Err(reason.clone());
        }
        self.ctl
            .send(Ctl::Frame(self.id, frame.into()))
            .map_err(|_| CloseReason::Exited("the kernel is gone".into()))
    }

    /// The next frame for this consumer, or why the connection ended.
    pub async fn recv(&mut self) -> Result<String, CloseReason> {
        if let Some(reason) = &self.closed {
            return Err(reason.clone());
        }
        let reason = match self.inbound.recv().await {
            Some(Inbound::Frame(frame)) => return Ok(frame),
            Some(Inbound::Closed(reason)) => reason,
            None => CloseReason::Exited("the kernel is gone".into()),
        };
        self.closed = Some(reason.clone());
        Err(reason)
    }

    /// The kernel generation this connection belongs to.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Why it ended, once `recv` said so.
    pub fn closed(&self) -> Option<&CloseReason> {
        self.closed.as_ref()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.core.detach(self.generation, self.id);
    }
}

// ---- the process's kernel --------------------------------------------------

fn global() -> &'static Core {
    static CORE: OnceLock<Core> = OnceLock::new();
    CORE.get_or_init(Core::default)
}

/// Configure the process's kernel (the shell, at startup, before the first
/// consumer connects). Later calls apply from the next start.
pub fn configure(options: Options) {
    global().configure(options)
}

/// The process kernel's core dir: `<core_dir>/profiles/_main.json` is the
/// profile the AI providers app writes.
pub fn core_dir() -> Option<PathBuf> {
    global().core_dir()
}

/// The process kernel's HOME (the parent of a `<home>/.octos` core dir).
pub fn home() -> Option<PathBuf> {
    core_dir().map(|d| kernel_home(&d))
}

/// Whether a kernel can run here, and how it would start.
pub fn launch() -> Result<Launch, Unavailable> {
    global().launch()
}

/// Whether a kernel can run here.
pub fn is_available() -> bool {
    launch().is_ok()
}

/// Connect to the process's kernel, starting it if needed.
pub fn connect() -> Result<Connection, Unavailable> {
    global().connect()
}

/// Restart the process's kernel if it runs (after a provider change).
pub fn restart() -> bool {
    global().restart()
}

/// Stop the process's kernel and wait (up to 5 s) for it to exit.
pub fn shutdown() -> bool {
    global().shutdown_within(Duration::from_secs(5))
}

/// The process kernel's state.
pub fn status() -> Status {
    global().status()
}

/// `<core_dir>/profiles/_main.json` of the process's kernel.
pub fn profile() -> Option<PathBuf> {
    core_dir().map(|d| profile_path(&d))
}
