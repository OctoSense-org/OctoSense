//! The octos kernel as a child process: where it lives, what it needs, and
//! how it is started — lifted from AppCard's `stdio_spawn` (app/app/src/main.rs).
//!
//! On Android the kernel is the `octos` CLI binary bundled into the APK as
//! `liboctos.so` (`MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=..."`, see
//! docs/android-appcard-build.md). An `untrusted_app` may exec only from its
//! nativeLibraryDir (W^X: a copy staged under app-writable storage dies with
//! `avc: denied { execute_no_trans }`), so that is the one place looked at,
//! and it is found by scanning `/proc/self/maps` for our own mapped
//! `libmakepad.so` — the directory carries a per-install hash and cannot be
//! hard-coded. The kernel runs `serve --stdio` with `HOME=<files>/octos-home`,
//! an app-private home whose `.config/octos/config.json` carries the memory
//! budget (`ensure_kernel_memory_budget`) and, once provisioned, the LLM key.
//!
//! On desktop the same contract points at an `octos` binary on `PATH` (or
//! `OCTOS_BIN`), with `HOME=<data dir>/octos-home` so the probe never takes
//! the data-dir lock of a developer's own `octos serve`.
//!
//! Foundation only: [`spawn_kernel`] starts the process and [`Kernel::probe`]
//! reports whether it came up. Nothing speaks the UI protocol over the pipes
//! yet — the shell track wires the transport. The reader threads and the
//! child process are a deliberate exception to the module contract's "no
//! processes, no threads" rule, the same exception Phase B makes for the
//! agent's tokio runtime: owned here, ended in `Drop`.

use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The kernel's HOME under the app's data directory.
pub const KERNEL_HOME_DIR: &str = "octos-home";
/// The name the bundled kernel binary has in the APK's native-lib dir.
pub const BUNDLED_KERNEL_LIB: &str = "liboctos.so";
/// `memory.max_inject_tokens` floor: octos's default (2500) silently
/// truncates the ~23k-token app-cards memory tree at inject time.
pub const INJECT_BUDGET_TOKENS: u64 = 40_000;
/// Set to `0` to skip the kernel entirely (tests, desktops without octos).
pub const PROBE_ENV: &str = "OCTOSENSE_APPCARD_KERNEL";

/// Whether the module should start a kernel at all.
pub fn probe_enabled() -> bool {
    !matches!(std::env::var(PROBE_ENV).as_deref(), Ok("0") | Ok("off") | Ok("false"))
}

/// `<data_dir>/octos-home`: the kernel's HOME.
pub fn kernel_home(data_dir: &Path) -> PathBuf {
    data_dir.join(KERNEL_HOME_DIR)
}

/// A running kernel. Killed and reaped when dropped.
pub struct Kernel {
    child: Child,
    program: PathBuf,
    home: PathBuf,
    stdout: mpsc::Receiver<String>,
    stderr: mpsc::Receiver<String>,
}

/// What a probe found out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeStatus {
    /// The kernel answered on stdout with this line.
    Spoke(String),
    /// No stdout within the wait, but the process is still up. `octos serve
    /// --stdio` says nothing until a `client_hello` arrives, so this is the
    /// expected outcome of a probe that sends nothing.
    Alive,
    /// The process ended before the wait was over.
    Exited { code: Option<i32>, stderr: String },
}

impl std::fmt::Display for ProbeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeStatus::Spoke(line) => write!(f, "ok (stdout: {line})"),
            ProbeStatus::Alive => write!(f, "ok (alive, silent until client_hello)"),
            ProbeStatus::Exited { code, stderr } => {
                write!(f, "error (exited with {code:?}")?;
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                write!(f, ")")
            }
        }
    }
}

impl Kernel {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    pub fn program(&self) -> &Path {
        &self.program
    }
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Wait up to `wait` for the first stdout line, then say how the process
    /// is doing. Blocks the caller for at most `wait`.
    pub fn probe(&mut self, wait: Duration) -> ProbeStatus {
        let deadline = Instant::now() + wait;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    let stderr = self.drain_stderr(Duration::from_millis(200));
                    return ProbeStatus::Exited { code: status.code(), stderr };
                }
                Ok(None) => {}
                Err(e) => {
                    return ProbeStatus::Exited { code: None, stderr: format!("try_wait: {e}") };
                }
            }
            let now = Instant::now();
            if now >= deadline {
                return ProbeStatus::Alive;
            }
            match self.stdout.recv_timeout((deadline - now).min(Duration::from_millis(100))) {
                Ok(line) => return ProbeStatus::Spoke(line),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // stdout closed: the process is going or gone. One more
                    // try_wait round settles which.
                    std::thread::sleep(Duration::from_millis(50));
                    if let Ok(Some(status)) = self.child.try_wait() {
                        let stderr = self.drain_stderr(Duration::from_millis(200));
                        return ProbeStatus::Exited { code: status.code(), stderr };
                    }
                    return ProbeStatus::Alive;
                }
            }
        }
    }

    /// Whatever stderr has said so far, joined; for error reports.
    fn drain_stderr(&self, grace: Duration) -> String {
        let mut lines = Vec::new();
        let deadline = Instant::now() + grace;
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match self.stderr.recv_timeout(deadline - now) {
                Ok(line) => lines.push(line),
                Err(_) => break,
            }
            if lines.len() >= 8 {
                break;
            }
        }
        lines.join(" | ")
    }

    /// Stop the kernel now. Idempotent.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Kernel {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Directory holding the app's packaged native libraries, found by scanning
/// `/proc/self/maps` for our own mapped `libmakepad.so`.
#[cfg(target_os = "android")]
pub fn native_lib_dir() -> Option<PathBuf> {
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    native_lib_dir_from_maps(&maps)
}

#[cfg(any(target_os = "android", test))]
fn native_lib_dir_from_maps(maps: &str) -> Option<PathBuf> {
    for line in maps.lines() {
        let Some(slash) = line.find('/') else { continue };
        let path = &line[slash..];
        if path.ends_with("/libmakepad.so") {
            return Path::new(path).parent().map(|p| p.to_path_buf());
        }
    }
    None
}

/// Where the kernel binary is. Android: the APK-bundled `liboctos.so` in the
/// nativeLibraryDir, and only that (see the module doc for why).
#[cfg(target_os = "android")]
pub fn locate_kernel() -> io::Result<PathBuf> {
    let lib_dir = native_lib_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "libmakepad.so not in /proc/self/maps; no nativeLibraryDir")
    })?;
    let program = lib_dir.join(BUNDLED_KERNEL_LIB);
    if program.exists() {
        Ok(program)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no {BUNDLED_KERNEL_LIB} under {} (build with MAKEPAD_ANDROID_EXTRA_LIBS)", lib_dir.display()),
        ))
    }
}

/// Where the kernel binary is. Desktop: `$OCTOS_BIN`, else `octos` on PATH.
#[cfg(not(target_os = "android"))]
pub fn locate_kernel() -> io::Result<PathBuf> {
    if let Some(bin) = std::env::var_os("OCTOS_BIN") {
        let bin = PathBuf::from(bin);
        if bin.is_file() {
            return Ok(bin);
        }
        return Err(io::Error::new(io::ErrorKind::NotFound, format!("OCTOS_BIN={} is not a file", bin.display())));
    }
    let exe = if cfg!(windows) { "octos.exe" } else { "octos" };
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe))
        .find(|p| p.is_file())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no `octos` on PATH (set OCTOS_BIN)"))
}

/// Ensure the kernel config (`<home>/.config/octos/config.json`) carries a
/// `memory.max_inject_tokens` big enough for the app-cards memory tree and
/// `appui.sessions_in_cwd = false`. Merge-only: every other key is preserved,
/// an explicit value at or above the floor wins, an unparseable file is left
/// alone (the kernel surfaces the parse error itself). Config file rather
/// than spawn env on purpose: env propagation on Android is not reliable
/// across process restarts.
pub fn ensure_kernel_memory_budget(home: &Path) -> io::Result<bool> {
    let path = home.join(".config/octos/config.json");
    let mut root = match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(v) if v.is_object() => v,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{} is not a JSON object; memory budget NOT ensured", path.display()),
                ))
            }
        },
        Err(_) => serde_json::json!({}),
    };
    let changed = merge_kernel_defaults(&mut root);
    if !changed {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let bytes = serde_json::to_vec_pretty(&root).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    std::fs::write(&path, bytes)?;
    Ok(true)
}

/// The merge itself, on the parsed document. Returns whether anything changed.
fn merge_kernel_defaults(root: &mut serde_json::Value) -> bool {
    let mut changed = false;
    let obj = root.as_object_mut().expect("caller checked is_object");
    {
        let memory = obj.entry("memory").or_insert_with(|| serde_json::json!({}));
        if let Some(memory) = memory.as_object_mut() {
            // Upgrade an ABSENT or too-LOW budget; a value at or above the
            // floor is an operator's tune and stays; a non-numeric value is
            // left alone.
            let too_low = memory
                .get("max_inject_tokens")
                .and_then(|v| v.as_f64())
                .map(|n| n < INJECT_BUDGET_TOKENS as f64)
                .unwrap_or(!memory.contains_key("max_inject_tokens"));
            if too_low {
                memory.insert("max_inject_tokens".into(), serde_json::json!(INJECT_BUDGET_TOKENS));
                changed = true;
            }
        }
    }
    {
        // The composer session is cwd-hinted into the app-cards memory tree;
        // without this the kernel relocates its transcripts into the card tree.
        let appui = obj.entry("appui").or_insert_with(|| serde_json::json!({}));
        if let Some(appui) = appui.as_object_mut() {
            if !appui.contains_key("sessions_in_cwd") {
                appui.insert("sessions_in_cwd".into(), serde_json::json!(false));
                changed = true;
            }
        }
    }
    changed
}

/// The environment the kernel runs with: an app-private HOME, the app-cards
/// memory dir as a skill read-zone, byte-stable prompts, an optional proxy
/// (`MAKEPAD_OCTOS_PROXY`, e.g. an `adb reverse` tunnel when the device has
/// no route out).
fn kernel_env(home: &Path) -> Vec<(String, String)> {
    let home_s = home.to_string_lossy().into_owned();
    let mut env = vec![
        ("HOME".to_owned(), home_s.clone()),
        ("OCTOS_SKILLS_PATH".to_owned(), home.join("a2app").to_string_lossy().into_owned()),
        ("RUST_LOG".to_owned(), "info".to_owned()),
        ("OCTOS_OMIT_WORKSPACE_HINT".to_owned(), "1".to_owned()),
    ];
    if let Ok(proxy) = std::env::var("MAKEPAD_OCTOS_PROXY") {
        let proxy = proxy.trim().to_owned();
        if !proxy.is_empty() {
            for k in ["HTTPS_PROXY", "HTTP_PROXY", "https_proxy", "http_proxy", "ALL_PROXY"] {
                env.push((k.to_owned(), proxy.clone()));
            }
        }
    }
    env
}

/// Start `octos serve --stdio` for the app whose data directory is
/// `data_dir` (Android: the files dir; desktop: whatever the host names).
/// The child's stdin, stdout and stderr are pipes; stdout and stderr are
/// read by threads into channels the [`Kernel`] keeps.
pub fn spawn_kernel(data_dir: &Path) -> io::Result<Kernel> {
    let program = locate_kernel()?;
    let home = kernel_home(data_dir);
    // HOME must exist BEFORE spawning: `Command::spawn` chdir's into `cwd`
    // before exec, so a missing octos-home fails with ENOENT even though the
    // binary is fine — permanently, since the server that would create it
    // never starts.
    std::fs::create_dir_all(&home)?;
    match ensure_kernel_memory_budget(&home) {
        Ok(true) => log!("kernel: set memory.max_inject_tokens={INJECT_BUDGET_TOKENS} in {}", home.display()),
        Ok(false) => {}
        Err(e) => log!("kernel: {e}"),
    }
    let mut cmd = Command::new(&program);
    cmd.args(["serve", "--stdio"])
        .envs(kernel_env(&home))
        .current_dir(&home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let stdout = line_reader(child.stdout.take().expect("piped stdout"));
    let stderr = line_reader(child.stderr.take().expect("piped stderr"));
    log!("kernel: spawned {} serve --stdio pid={} HOME={}", program.display(), child.id(), home.display());
    Ok(Kernel { child, program, home, stdout, stderr })
}

fn line_reader<R: io::Read + Send + 'static>(reader: R) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("appcard-kernel-pipe".into())
        .spawn(move || {
            for line in BufReader::new(reader).lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        })
        .expect("spawn kernel pipe reader");
    rx
}

// `log!` is makepad's; on Android it lands in logcat under the `Makepad` tag.
use makepad_widgets::log;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_lib_dir_comes_from_the_mapped_libmakepad() {
        let maps = "7f0000-7f1000 r-xp 00000000 fd:00 1 /data/app/~~x==/dev.makepad.octosense-y==/lib/arm64/libmakepad.so\n\
                    7f2000-7f3000 r--p 00000000 00:00 0 [anon]\n";
        assert_eq!(
            native_lib_dir_from_maps(maps).as_deref(),
            Some(Path::new("/data/app/~~x==/dev.makepad.octosense-y==/lib/arm64"))
        );
        assert_eq!(native_lib_dir_from_maps("nothing here"), None);
    }

    #[test]
    fn the_memory_budget_merge_upgrades_low_values_and_keeps_tunes() {
        let mut v = serde_json::json!({});
        assert!(merge_kernel_defaults(&mut v));
        assert_eq!(v["memory"]["max_inject_tokens"], serde_json::json!(INJECT_BUDGET_TOKENS));
        assert_eq!(v["appui"]["sessions_in_cwd"], serde_json::json!(false));
        // A pre-app-cards default (2500, int or float) is upgraded.
        let mut v = serde_json::json!({"memory": {"max_inject_tokens": 2500.0}, "appui": {"sessions_in_cwd": true}, "other": 1});
        assert!(merge_kernel_defaults(&mut v));
        assert_eq!(v["memory"]["max_inject_tokens"], serde_json::json!(INJECT_BUDGET_TOKENS));
        assert_eq!(v["appui"]["sessions_in_cwd"], serde_json::json!(true), "an explicit operator value wins");
        assert_eq!(v["other"], serde_json::json!(1), "every other key is preserved");
        // An operator's deliberate tune at or above the floor stays.
        let mut v = serde_json::json!({"memory": {"max_inject_tokens": 60000}, "appui": {"sessions_in_cwd": false}});
        assert!(!merge_kernel_defaults(&mut v));
        assert_eq!(v["memory"]["max_inject_tokens"], serde_json::json!(60000));
    }

    #[test]
    fn the_budget_is_written_into_a_fresh_home_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("octosense-appcard-kernel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let home = kernel_home(&dir);
        std::fs::create_dir_all(&home).unwrap();
        assert!(ensure_kernel_memory_budget(&home).unwrap());
        assert!(!ensure_kernel_memory_budget(&home).unwrap());
        let text = std::fs::read_to_string(home.join(".config/octos/config.json")).unwrap();
        assert!(text.contains("\"max_inject_tokens\": 40000"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn probe_status_prints_the_verdict_first() {
        assert!(ProbeStatus::Alive.to_string().starts_with("ok"));
        assert!(ProbeStatus::Spoke("x".into()).to_string().starts_with("ok"));
        assert!(ProbeStatus::Exited { code: Some(1), stderr: "boom".into() }.to_string().starts_with("error"));
    }
}
