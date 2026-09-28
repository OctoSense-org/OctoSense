//! One kernel generation: start it, carry frames between it and the
//! consumers through the [`Router`], stop it, tell the consumers why.

use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, watch};

use crate::launch::Launch;
use crate::router::{ConnId, Router};
use crate::network::Network;
use crate::{ClientAccess, CloseReason, LogSink};
use std::path::PathBuf;

/// What a consumer's inbound channel carries.
#[derive(Debug)]
pub(crate) enum Inbound {
    Frame(String),
    Closed(CloseReason),
}

/// Messages to a generation's supervisor.
pub(crate) enum Ctl {
    Attach(ConnId, mpsc::UnboundedSender<Inbound>),
    Frame(ConnId, String),
    Detach(ConnId),
    Stop(CloseReason),
}

/// How long a stopping kernel may drain before it is killed.
const STDIO_GRACE: Duration = Duration::from_secs(3);
/// A host-managed server drains open WebSocket connections for up to 10 s
/// after its stdin closes (octos `SERVE_SHUTDOWN_GRACE`).
const SHARED_GRACE: Duration = Duration::from_secs(12);
/// The embedded core drains owned turns on EOF (as AppCard allowed it).
#[cfg(target_env = "ohos")]
const EMBEDDED_GRACE: Duration = Duration::from_secs(12);

enum Running {
    Child(tokio::process::Child),
    #[cfg(target_env = "ohos")]
    Embedded(tokio::task::JoinHandle<()>),
}

struct Io {
    writer: Pin<Box<dyn AsyncWrite + Send>>,
    lines: tokio::io::Lines<BufReader<Pin<Box<dyn AsyncRead + Send>>>>,
    running: Running,
    /// Talk to Octos: the child's stdin, held open for its whole life. The
    /// host-managed server stops when it reaches EOF, which also happens
    /// when this process dies, so a crashed shell leaves no kernel behind.
    lifeline: Option<tokio::process::ChildStdin>,
}

/// Keeps the last lines the kernel wrote to stderr, to say why it exited.
#[derive(Clone, Default)]
struct Tail(Arc<std::sync::Mutex<VecDeque<String>>>);

impl Tail {
    fn push(&self, line: String) {
        let mut t = self.0.lock().unwrap();
        if t.len() == 4 {
            t.pop_front();
        }
        t.push_back(line);
    }
    fn text(&self) -> String {
        self.0.lock().unwrap().iter().cloned().collect::<Vec<_>>().join(" | ")
    }
}

fn start(launch: &Launch, network: &Network, log: &LogSink, tail: &Tail) -> Result<Io, String> {
    match launch {
        Launch::Stdio { program, args, env, cwd } | Launch::WebSocket { program, args, env, cwd } => {
            let shared = matches!(launch, Launch::WebSocket { .. });
            let mut command = tokio::process::Command::new(program);
            command
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            for (k, v) in env {
                command.env(k, v);
            }
            let mut secrets = Vec::new();
            if shared {
                let external = network.external_token();
                command
                    .env("OCTOS_AUTH_TOKEN", network.host_token())
                    .env("OCTOS_HOST_EXTERNAL_TOKEN", &external)
                    .env("NO_COLOR", "1")
                    .env_remove("OCTOS_INSTANCE_DATA_DIR")
                    .env_remove("OCTOS_SOLO_LOGIN");
                secrets = vec![network.host_token().to_owned(), external];
                pass_listener(&mut command, network)?;
            }
            if let Some(cwd) = cwd {
                command.current_dir(cwd);
            }
            let mut child = command.spawn().map_err(|e| format!("spawn {}: {e}", program.display()))?;
            let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
                let _ = child.start_kill();
                return Err("the kernel's stdin/stdout are not piped".into());
            };
            // The kernel logs to stderr; it never carries protocol frames.
            if let Some(stderr) = child.stderr.take() {
                let log = log.clone();
                let tail = tail.clone();
                tokio::spawn(async move {
                    let mut lines = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let line = secrets.iter().fold(line, |line, secret| line.replace(secret.as_str(), "[redacted]"));
                        (log)(&format!("octos: {line}"));
                        tail.push(line);
                    }
                });
            }
            let lines = BufReader::new(Box::pin(stdout) as Pin<Box<dyn AsyncRead + Send>>).lines();
            Ok(if shared {
                // Frames go over the WebSocket once it is up (`prepare_network`).
                Io { writer: Box::pin(tokio::io::sink()), lines, running: Running::Child(child), lifeline: Some(stdin) }
            } else {
                Io { writer: Box::pin(stdin), lines, running: Running::Child(child), lifeline: None }
            })
        }
        #[cfg(target_env = "ohos")]
        Launch::Embedded { home } => {
            let (client, server) = tokio::io::duplex(1024 * 1024);
            let (reader, writer) = tokio::io::split(client);
            let (server_reader, server_writer) = tokio::io::split(server);
            let home = home.clone();
            init_embedded_tracing(log);
            let log = log.clone();
            let task = tokio::spawn(async move {
                let result = octos_cli::embedded::serve_io(&home, server_reader, server_writer).await;
                (log)(&format!("octos-core: embedded core stopped: {result:?}"));
            });
            Ok(Io {
                writer: Box::pin(writer),
                lines: BufReader::new(Box::pin(reader) as Pin<Box<dyn AsyncRead + Send>>).lines(),
                running: Running::Embedded(task),
                lifeline: None,
            })
        }
        #[cfg(not(target_env = "ohos"))]
        Launch::Embedded { .. } => Err("an embedded core exists only on OpenHarmony".into()),
    }
}


/// Unix: hand the listener this process keeps to the child as descriptor 3
/// (`--listen-fd 3`). Elsewhere: `--port`, the previous one when still free.
#[cfg(unix)]
fn pass_listener(command: &mut tokio::process::Command, network: &Network) -> Result<(), String> {
    let fd = network.listener_fd().map_err(|e| format!("cannot open the Talk to Octos listener: {e}"))?;
    command.args(["--listen-fd", "3"]);
    // SAFETY: between fork and exec only async-signal-safe dup2/fcntl run.
    // They place the kept listener at descriptor 3 and clear its
    // close-on-exec flag there (dup2 onto itself would keep the flag).
    unsafe {
        command.pre_exec(move || {
            if fd != 3 && libc::dup2(fd, 3) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::fcntl(3, libc::F_SETFD, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn pass_listener(command: &mut tokio::process::Command, network: &Network) -> Result<(), String> {
    command.args(["--port", &network.port_for_restart().to_string()]);
    Ok(())
}

async fn prepare_network(
    io: &mut Io, network: &Network, core_dir: &std::path::Path,
    ready: &watch::Sender<Option<Result<ClientAccess, CloseReason>>>,
) -> Result<(), String> {
    if io.lifeline.is_none() {
        // The private pipe (Talk to Octos off, or the embedded core).
        ready.send_replace(Some(Err(CloseReason::Failed("Talk to Octos is off.".into()))));
        return Ok(());
    }
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let line = io.lines.next_line().await.map_err(|e| e.to_string())?
                .ok_or_else(|| "the kernel exited before opening its WebSocket listener (it needs an octos with `serve --host-managed`)".to_owned())?;
            if network.announced(&line).is_some() { break; }
        }
        let access = network.access();
        let stream = crate::network::connect(&access, network.host_token()).await?;
        let (reader, writer) = tokio::io::split(stream);
        let stdout = std::mem::replace(&mut io.lines, BufReader::new(Box::pin(reader) as Pin<Box<dyn AsyncRead + Send>>).lines());
        io.writer = Box::pin(writer);
        // Keep draining the child's stdout; nothing more is parsed from it.
        tokio::spawn(async move { let mut lines = stdout; while let Ok(Some(_)) = lines.next_line().await {} });
        access.save(core_dir).map_err(|e| format!("cannot save the private client connection: {e}"))?;
        ready.send_replace(Some(Ok(access)));
        Ok(())
    }).await.map_err(|_| "the kernel did not become ready within 90 seconds".to_owned())?
}

/// The embedded core logs through `tracing`; send it to the log sink, once
/// per process (as AppCard's embedded transport did).
#[cfg(target_env = "ohos")]
fn init_embedded_tracing(log: &LogSink) {
    struct Sink(LogSink);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            (self.0)(String::from_utf8_lossy(bytes).trim_end());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let log = log.clone();
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,reqwest=warn,hyper=warn,html5ever=warn,octos.prompt_cache=trace")
        .with_ansi(false)
        .with_writer(move || Sink(log.clone()))
        .try_init();
}

/// Wait for the kernel to end on its own.
async fn exited(running: &mut Running) -> String {
    match running {
        Running::Child(child) => match child.wait().await {
            Ok(status) => format!("the kernel exited ({status})"),
            Err(e) => format!("waiting for the kernel failed: {e}"),
        },
        #[cfg(target_env = "ohos")]
        Running::Embedded(task) => match task.await {
            Ok(()) => "the embedded core stopped".into(),
            Err(e) => format!("the embedded core failed: {e}"),
        },
    }
}

/// Stop the kernel: close its input (it drains owned turns on EOF), wait a
/// grace period, then kill it and wait, so its data-dir lock is released
/// before a next generation starts.
async fn stop(io: Io) {
    // Closing stdin stops either kind of child: the stdio pipe drains owned
    // turns on EOF; a host-managed server drains its connections and exits.
    let Io { writer, lines, mut running, lifeline } = io;
    let grace = if lifeline.is_some() { SHARED_GRACE } else { STDIO_GRACE };
    drop(writer);
    drop(lines);
    drop(lifeline);
    match &mut running {
        Running::Child(child) => {
            if tokio::time::timeout(grace, child.wait()).await.is_err() {
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
        }
        #[cfg(target_env = "ohos")]
        Running::Embedded(task) => {
            if !task.is_finished() && tokio::time::timeout(EMBEDDED_GRACE, &mut *task).await.is_err() {
                task.abort();
            }
        }
    }
}

/// Settings that remain fixed for one kernel generation.
pub(crate) struct GenerationConfig {
    pub generation: u64,
    pub launch: Launch,
    pub network: Arc<Network>,
    pub core_dir: PathBuf,
    pub log: LogSink,
}

/// Run a generation until it is stopped or its kernel ends.
pub(crate) async fn supervise(
    config: GenerationConfig,
    ready: watch::Sender<Option<Result<ClientAccess, CloseReason>>>,
    mut ctl: mpsc::UnboundedReceiver<Ctl>,
    done: watch::Sender<bool>,
    ended: impl FnOnce() + Send,
) {
    let GenerationConfig { generation, launch, network, core_dir, log } = config;
    let mut consumers: HashMap<ConnId, mpsc::UnboundedSender<Inbound>> = HashMap::new();
    let mut router = Router::new(&core_dir);
    let tail = Tail::default();
    (log)(&format!("octos-core: starting kernel {generation}: {}", describe(&launch)));
    let started = start(&launch, &network, &log, &tail);
    let reason = match started {
        Err(e) => {
            (log)(&format!("octos-core: kernel {generation} did not start: {e}"));
            // Consumers that attached while we waited learn why below;
            // the ones still queued are drained there too.
            CloseReason::Failed(e)
        }
        Ok(mut io) => {
            let mut queued = VecDeque::new();
            // Keep ownership of the child while waiting for startup so Stop
            // can always kill AND reap it before the next generation starts.
            let startup = {
                let setup = prepare_network(&mut io, &network, &core_dir, &ready);
                tokio::pin!(setup);
                loop {
                    tokio::select! {
                        biased;
                        msg = ctl.recv() => match msg {
                            Some(Ctl::Attach(id, tx)) => { router.attach(id); consumers.insert(id, tx); }
                            Some(Ctl::Detach(id)) => { router.detach(id); consumers.remove(&id); }
                            Some(Ctl::Stop(reason)) => break Err(reason),
                            Some(frame) => queued.push_back(frame),
                            None => break Err(CloseReason::Shutdown),
                        },
                        result = &mut setup => break result.map_err(CloseReason::Failed),
                    }
                }
            };
            let reason = if let Err(reason) = startup { reason } else { loop {
                tokio::select! {
                    biased;
                    msg = async { if queued.is_empty() { ctl.recv().await } else { queued.pop_front() } } => match msg {
                        Some(Ctl::Attach(id, tx)) => {
                            router.attach(id);
                            consumers.insert(id, tx);
                        }
                        Some(Ctl::Frame(id, text)) => {
                            if let Some(frame) = router.consumer_frame(id, &text) {
                                let write = async {
                                    io.writer.write_all(frame.as_bytes()).await?;
                                    io.writer.write_all(b"\n").await?;
                                    io.writer.flush().await
                                };
                                if let Err(e) = write.await {
                                    break CloseReason::Exited(format!("writing to the kernel failed: {e}"));
                                }
                            }
                        }
                        Some(Ctl::Detach(id)) => {
                            router.detach(id);
                            consumers.remove(&id);
                        }
                        Some(Ctl::Stop(reason)) => break reason,
                        None => break CloseReason::Shutdown,
                    },
                    line = io.lines.next_line() => match line {
                        Ok(Some(text)) => {
                            if text.trim().is_empty() {
                                continue;
                            }
                            for (id, frame) in router.kernel_frame(&text) {
                                if let Some(tx) = consumers.get(&id) {
                                    let _ = tx.send(Inbound::Frame(frame));
                                }
                            }
                        }
                        Ok(None) => break CloseReason::Exited(with_tail("the kernel closed its output", &tail)),
                        Err(e) => break CloseReason::Exited(format!("reading from the kernel failed: {e}")),
                    },
                    why = exited(&mut io.running) => break CloseReason::Exited(with_tail(&why, &tail)),
                }
            }};
            (log)(&format!("octos-core: stopping kernel {generation}: {reason}"));
            stop(io).await;
            match reason {
                CloseReason::Failed(why) => CloseReason::Failed(with_tail(&why, &tail)),
                other => other,
            }
        }
    };
    let was_ready = matches!(&*ready.borrow(), Some(Ok(_)));
    ready.send_replace(Some(Err(reason.clone())));
    if matches!(launch, Launch::WebSocket { .. }) {
        crate::network::remove_descriptor(&core_dir);
        // Without an inherited listener, a server that never came up may
        // have lost its port to another process: the next start takes a new
        // port and a new external token.
        if !was_ready && !matches!(reason, CloseReason::Restarted | CloseReason::Shutdown) && cfg!(not(unix)) {
            network.fall_back();
        }
    }
    // Every consumer of this generation learns why it ended — including one
    // whose Attach is still queued.
    ctl.close();
    while let Ok(msg) = ctl.try_recv() {
        if let Ctl::Attach(id, tx) = msg {
            consumers.insert(id, tx);
        }
    }
    for tx in consumers.values() {
        let _ = tx.send(Inbound::Closed(reason.clone()));
    }
    ended();
    let _ = done.send(true);
}

fn with_tail(why: &str, tail: &Tail) -> String {
    let tail = tail.text();
    if tail.is_empty() {
        why.to_owned()
    } else {
        format!("{why}: {tail}")
    }
}

fn describe(launch: &Launch) -> String {
    match launch {
        Launch::Stdio { program, args, .. } | Launch::WebSocket { program, args, .. } => format!("{} {}", program.display(), args.join(" ")),
        Launch::Embedded { home } => format!("embedded core, home {}", home.display()),
    }
}
