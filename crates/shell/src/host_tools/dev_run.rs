//! `dev.run {command, cwd?, timeout_ms?}`: developer mode's command tool
//! (ADR 0004 §13), a host tool the shell executes.
//!
//! - **Registered** only on the peers of apps developer mode covers
//!   ([`super::relay::Catalog::offered`]), declared as the app's own tool
//!   (`app` is the calling app), so the router's developer-mode answer and
//!   the audit are keyed to that app. The broker registers peers on the
//!   shell's host connection only, so a Talk to Octos client never sees it
//!   (octos also keeps host-routed tools out of external turns, #2601); the
//!   system chat's session never registers it
//!   (`system_chat::grants::host_tools`).
//! - **Withdrawn** when developer mode ends: every live peer registers its
//!   tools again ([`super::developer_mode_changed`]), and the relay refuses a
//!   late call whose app developer mode no longer covers.
//! - **Runs** here: `sh -c` (`cmd /C` on Windows), in the agent's workspace
//!   (its account folder) unless `cwd` names another directory, killed after
//!   `timeout_ms` (at most the kernel's call timeout and [`MAX_TIMEOUT_MS`]),
//!   stdout and stderr each capped at [`OUTPUT_CAP`] bytes.
//! - **Audited**: every command and its exit (`dev_mode::audit_dev_run`), and
//!   its automatic approval by the router (`confirm: host`, a command).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::ai_host::app_peers::host_tools::{HostToolCall, ToolExecutor, ToolOutcome, ToolReply};

use super::relay::DEV_RUN;

/// Each of stdout and stderr is kept up to this many bytes.
pub const OUTPUT_CAP: usize = 64 * 1024;
/// A command runs at most this long, whatever it asks.
pub const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1000;
/// Without `timeout_ms`: this, or the kernel's call timeout if shorter.
pub const DEFAULT_TIMEOUT_MS: u64 = 60 * 1000;

/// `dev.run`'s declaration, as `app`'s own tool (a host tool the shell runs).
pub fn declaration(app: &str) -> Value {
    json!({
        "name": DEV_RUN,
        "app": app,
        "description": "Developer mode only: run a shell command on this machine and return its exit code, stdout and stderr (each capped at 64 KiB). Runs in your workspace unless `cwd` names another directory; killed after `timeout_ms` (default 60 s, at most 10 min). Every command is logged.",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {"type": "string", "minLength": 1, "maxLength": 16384},
                "cwd": {"type": "string", "maxLength": 4096},
                "timeout_ms": {"type": "integer", "minimum": 1, "maximum": MAX_TIMEOUT_MS}
            },
            "required": ["command"],
            "additionalProperties": false
        },
        "risk": "destructive",
        "confirm": "host",
        "shareable": false
    })
}

/// What a command did.
#[derive(Clone, Debug, PartialEq)]
pub struct Ran {
    /// `None`: killed (timeout, cancel) or ended by a signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    pub timed_out: bool,
    pub cancelled: bool,
}

impl Ran {
    pub fn to_json(&self, cwd: &Path) -> Value {
        json!({
            "exit_code": self.exit_code,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "truncated": self.truncated,
            "timed_out": self.timed_out,
            "cwd": cwd.to_string_lossy(),
        })
    }
}

fn shell(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    }
}

/// Read at most `cap` bytes and drain the rest (so the child never blocks).
fn capped(mut from: impl Read + Send + 'static, cap: usize) -> std::thread::JoinHandle<(Vec<u8>, bool)> {
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut truncated = false;
        let mut buf = [0u8; 8192];
        loop {
            match from.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = cap.saturating_sub(kept.len());
                    if n > room {
                        truncated = true;
                    }
                    kept.extend_from_slice(&buf[..n.min(room)]);
                }
            }
        }
        (kept, truncated)
    })
}

fn kill(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        // The whole process group: `sh -c` and whatever it started.
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
}

/// Run `command` in `cwd`, for at most `timeout`, stopping early when
/// `cancel` is set.
pub fn run(command: &str, cwd: &Path, timeout: Duration, cap: usize, cancel: &AtomicBool) -> Result<Ran, String> {
    let mut cmd = shell(command);
    cmd.current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| format!("could not start the command in {}: {e}", cwd.display()))?;
    let out = capped(child.stdout.take().expect("piped"), cap);
    let err = capped(child.stderr.take().expect("piped"), cap);
    let started = Instant::now();
    let (mut timed_out, mut cancelled) = (false, false);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        if cancel.load(Ordering::SeqCst) {
            cancelled = true;
        } else if started.elapsed() >= timeout {
            timed_out = true;
        }
        if cancelled || timed_out {
            kill(&mut child);
            break child.wait().ok();
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let (stdout, t1) = out.join().unwrap_or_default();
    let (stderr, t2) = err.join().unwrap_or_default();
    let exit_code = if timed_out || cancelled { None } else { status.and_then(|s| s.code()) };
    Ok(Ran {
        exit_code,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        truncated: t1 || t2,
        timed_out,
        cancelled,
    })
}

/// The command's working directory: `cwd` (relative to the workspace, or
/// absolute), or the workspace itself.
pub fn resolve_cwd(workspace: Option<&Path>, cwd: Option<&str>) -> Result<PathBuf, String> {
    let dir = match (cwd.filter(|c| !c.is_empty()), workspace) {
        (Some(c), _) if Path::new(c).is_absolute() => PathBuf::from(c),
        (Some(c), Some(w)) => w.join(c),
        (None, Some(w)) => w.to_path_buf(),
        (_, None) => return Err("this agent has no workspace; pass an absolute cwd".into()),
    };
    if !dir.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    Ok(dir)
}

/// How long a call may run: its own ask, within the kernel's call timeout.
pub fn timeout_for(asked: Option<u64>, kernel_ms: u64) -> Duration {
    let ceiling = if kernel_ms == 0 { MAX_TIMEOUT_MS } else { kernel_ms.min(MAX_TIMEOUT_MS) };
    Duration::from_millis(asked.unwrap_or(DEFAULT_TIMEOUT_MS).min(ceiling))
}

/// The shell's executor for `dev.run` (each call on its own thread).
#[derive(Default)]
pub struct DevRunExecutor {
    /// Running calls' cancel flags.
    running: Mutex<Vec<(String, Arc<AtomicBool>)>>,
    /// The agent's workspace for (peer app id, account); the shell's is
    /// `super::agent_workspace`.
    workspace: Option<fn(&str, &str) -> Option<PathBuf>>,
}

impl DevRunExecutor {
    pub fn new(workspace: fn(&str, &str) -> Option<PathBuf>) -> DevRunExecutor {
        DevRunExecutor { running: Mutex::default(), workspace: Some(workspace) }
    }
}

impl ToolExecutor for DevRunExecutor {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        let command = call.args["command"].as_str().unwrap_or("").to_string();
        let workspace = self.workspace.and_then(|w| w(&call.calling_app, call.account.as_deref().unwrap_or("device")));
        let cwd = match resolve_cwd(workspace.as_deref(), call.args["cwd"].as_str()) {
            Ok(dir) => dir,
            Err(why) => {
                crate::dev_mode::audit_dev_run(&call.app, &command, call.args["cwd"].as_str(), None);
                reply.finish(ToolOutcome::error("invalid_args", why));
                return;
            }
        };
        let timeout = timeout_for(call.args["timeout_ms"].as_u64(), call.timeout_ms);
        let cancel = Arc::new(AtomicBool::new(false));
        self.running.lock().unwrap_or_else(|e| e.into_inner()).push((call.call_id.clone(), cancel.clone()));
        let app = call.app.clone();
        std::thread::spawn(move || {
            let ran = run(&command, &cwd, timeout, OUTPUT_CAP, &cancel);
            let cwd_text = cwd.to_string_lossy().into_owned();
            match ran {
                Ok(ran) => {
                    crate::dev_mode::audit_dev_run(&app, &command, Some(&cwd_text), ran.exit_code);
                    if !ran.cancelled {
                        reply.finish(ToolOutcome::Ok(ran.to_json(&cwd)));
                    }
                }
                Err(why) => {
                    crate::dev_mode::audit_dev_run(&app, &command, Some(&cwd_text), None);
                    reply.finish(ToolOutcome::error("app_error", why));
                }
            }
        });
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        running.retain(|(_, flag)| Arc::strong_count(flag) > 1);
    }

    fn cancel(&self, call_id: &str) {
        let running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        for (id, flag) in running.iter() {
            if id == call_id {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("octosense-devrun-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_command_runs_in_its_directory_with_its_exit_code_and_output() {
        let dir = tmp("basic");
        let ran = run("pwd; echo oops >&2; exit 3", &dir, Duration::from_secs(10), OUTPUT_CAP, &AtomicBool::new(false)).unwrap();
        assert_eq!(ran.exit_code, Some(3));
        assert_eq!(std::fs::canonicalize(ran.stdout.trim()).unwrap(), std::fs::canonicalize(&dir).unwrap());
        assert_eq!(ran.stderr, "oops\n");
        assert!(!ran.truncated && !ran.timed_out);
    }

    #[test]
    fn output_is_capped_and_a_slow_command_is_killed_at_its_timeout() {
        let dir = tmp("cap");
        let ran = run("head -c 100000 /dev/zero | tr '\\0' x", &dir, Duration::from_secs(10), 1000, &AtomicBool::new(false)).unwrap();
        assert_eq!(ran.stdout.len(), 1000);
        assert!(ran.truncated);
        let started = Instant::now();
        let ran = run("sleep 30", &dir, Duration::from_millis(200), OUTPUT_CAP, &AtomicBool::new(false)).unwrap();
        assert!(ran.timed_out && ran.exit_code.is_none());
        assert!(started.elapsed() < Duration::from_secs(5), "killed, not waited for");
    }

    #[test]
    fn the_working_directory_is_the_workspace_unless_cwd_names_another() {
        let ws = tmp("ws");
        std::fs::create_dir_all(ws.join("sub")).unwrap();
        assert_eq!(resolve_cwd(Some(&ws), None).unwrap(), ws);
        assert_eq!(resolve_cwd(Some(&ws), Some("sub")).unwrap(), ws.join("sub"));
        let abs = std::env::temp_dir();
        assert_eq!(resolve_cwd(Some(&ws), Some(abs.to_str().unwrap())).unwrap(), abs);
        assert!(resolve_cwd(Some(&ws), Some("missing")).is_err());
        assert!(resolve_cwd(None, None).is_err(), "no workspace, no relative cwd");
        assert_eq!(timeout_for(None, 30_000), Duration::from_millis(30_000), "within the kernel's call timeout");
        assert_eq!(timeout_for(Some(5), 30_000), Duration::from_millis(5));
        assert_eq!(timeout_for(Some(u64::MAX), 0), Duration::from_millis(MAX_TIMEOUT_MS));
    }
}
