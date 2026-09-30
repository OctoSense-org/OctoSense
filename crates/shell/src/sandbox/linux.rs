//! The Linux sandbox: Landlock for paths (and, from ABI 4, TCP ports) and a
//! seccomp filter, both installed in the child between fork and exec.
//!
//! Landlock is an allow-list: everything the ruleset handles is closed
//! except the rules, so the person's home is closed without naming it.
//! The rules: the system's read-only locations (libraries, `/etc`, `/usr`),
//! devices (the GPU), `/proc` and `/sys`, the app's program and resources
//! read-only, and its jail, secrets and `external` grants. Execution is
//! allowed only from the program's roots (and the system's, when
//! `processes` is true).
//!
//! **The host's private directories** ([`Policy::private`]: the OctoSense
//! home, the kernel's core dir) stay closed whatever a grant opens. Landlock
//! cannot deny beneath an allowed directory, so a grant that contains one
//! (the Terminal's `home:rw`) is split: each entry of the granted directory
//! is granted on its own, recursing only into directories on the way to a
//! private one, which get no right at all. The app's own jail and secrets
//! are granted by themselves. The limits of that split: entries created in
//! a split directory after the app started (a new file directly in `~`) are
//! not covered, and the directories on the way cannot be listed.
//!
//! seccomp refuses, with `EPERM`, what no process app needs: `ptrace`,
//! `process_vm_readv/writev`, `perf_event_open`, `bpf`, `userfaultfd`,
//! `kexec_load`, mounts, namespaces and the kernel keyring; with
//! `processes: false` also `fork`, `vfork` and a `clone` that is not a
//! thread (and `clone3`, which libc then retries as `clone`).
//!
//! **Best-effort.** The parent probes the kernel's Landlock ABI before the
//! spawn and says in [`Applied`] which layers took (a kernel before 5.13, or
//! one without Landlock enabled, gets seccomp only; before ABI 4, no port
//! rules). A layer that fails in the child is skipped, never fatal: the app
//! still starts.
//!
//! **A launch through cargo** (a dev run from a checkout) sandboxes only the
//! app, as macOS does: cargo builds outside the sandbox and starts the app
//! through its `runner`, which is the shell's own binary in runner mode
//! ([`RUNNER_FLAG`], entered before `main` by [`runner_entry!`]): it reads the
//! [`Plan`] prepared here, installs it on itself and execs the app. A build
//! inside the sandbox needed the checkout, the target dir, cargo's home and
//! `/tmp` writable, which let the app rewrite the shell's binary, the source
//! and `~/.cargo` (found on a real kernel, #138).

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use super::{Access, Applied, Policy};
use crate::native_apps::Network;

const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;
const SYS_LANDLOCK_ADD_RULE: libc::c_long = 445;
const SYS_LANDLOCK_RESTRICT_SELF: libc::c_long = 446;
const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;
const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;
const LANDLOCK_RULE_NET_PORT: u32 = 2;

const FS_EXECUTE: u64 = 1 << 0;
const FS_WRITE_FILE: u64 = 1 << 1;
const FS_READ_FILE: u64 = 1 << 2;
const FS_READ_DIR: u64 = 1 << 3;
const FS_TRUNCATE: u64 = 1 << 14;
const FS_IOCTL_DEV: u64 = 1 << 15;
const NET_BIND_TCP: u64 = 1 << 0;
const NET_CONNECT_TCP: u64 = 1 << 1;

/// The rights a file (not a directory) may carry in a rule.
const FILE_RIGHTS: u64 = FS_EXECUTE | FS_WRITE_FILE | FS_READ_FILE | FS_TRUNCATE | FS_IOCTL_DEV;

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
}

#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

#[repr(C)]
struct NetPortAttr {
    allowed_access: u64,
    port: u64,
}

/// The kernel's Landlock ABI version, 0 when it has none.
pub fn landlock_abi() -> u32 {
    let r = unsafe { libc::syscall(SYS_LANDLOCK_CREATE_RULESET, std::ptr::null::<RulesetAttr>(), 0usize, LANDLOCK_CREATE_RULESET_VERSION) };
    if r < 0 { 0 } else { r as u32 }
}

/// Every filesystem right the ruleset handles at `abi`.
pub fn handled_fs(abi: u32) -> u64 {
    let mut rights = (1u64 << 13) - 1; // ABI 1: EXECUTE ..= MAKE_SYM
    if abi >= 2 {
        rights |= 1 << 13; // REFER
    }
    if abi >= 3 {
        rights |= FS_TRUNCATE;
    }
    if abi >= 5 {
        rights |= FS_IOCTL_DEV;
    }
    rights
}

/// One path rule, prepared in the parent.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub path: PathBuf,
    pub access: u64,
}

/// Read and execute: what a program path keeps inside the private dirs.
pub fn read_exec() -> u64 {
    read() | FS_EXECUTE
}

fn read() -> u64 {
    FS_READ_FILE | FS_READ_DIR
}

/// The path rules for `policy` at `abi`.
pub fn rules(policy: &Policy, abi: u32) -> Vec<Rule> {
    let all = handled_fs(abi);
    let rx = read() | FS_EXECUTE;
    let system_exec = if policy.processes { rx } else { read() };
    let mut out = Vec::new();
    let mut add = |path: PathBuf, access: u64| {
        if path.as_os_str().is_empty() {
            return;
        }
        let access = access & all;
        let access = if path.is_dir() { access } else { access & FILE_RIGHTS };
        out.push(Rule { path, access });
    };
    // Shared libraries are mapped, not executed: read is enough unless the
    // app may start other programs.
    for dir in ["/usr", "/lib", "/lib64", "/lib32", "/bin", "/sbin", "/opt", "/nix/store"] {
        add(PathBuf::from(dir), system_exec);
    }
    // The dynamic loader is exec'd with the program.
    for loader in ["/lib64/ld-linux-x86-64.so.2", "/lib/ld-linux-aarch64.so.1"] {
        add(PathBuf::from(loader), rx);
    }
    add(PathBuf::from("/etc"), read());
    add(PathBuf::from("/sys"), read());
    add(PathBuf::from("/proc"), read() | FS_WRITE_FILE);
    add(PathBuf::from("/dev"), read() | FS_WRITE_FILE | FS_IOCTL_DEV);
    add(PathBuf::from("/run"), read());
    if let Some(home) = super::person_home() {
        for fonts in [".local/share/fonts", ".fonts", ".cache/fontconfig", ".config/fontconfig"] {
            add(home.join(fonts), read());
        }
    }
    for program in &policy.program {
        add(program.clone(), rx);
    }
    add(policy.jail.clone(), all);
    add(policy.secrets.clone(), all);
    for (path, access) in &policy.external {
        add(path.clone(), if *access == Access::Read { read() } else { all });
    }
    // Compare real paths: Landlock follows links when it opens a rule's
    // path, so a linked checkout or grant must not slip past the private
    // directories by its spelling.
    let private: Vec<PathBuf> = policy.private.iter().map(|p| super::resolved(p)).collect();
    let program: Vec<PathBuf> = policy.program.iter().map(|p| super::resolved(p)).collect();
    let own = [super::resolved(&policy.jail), super::resolved(&policy.secrets)];
    let mut split = Vec::new();
    for rule in out {
        let rule = Rule { path: super::resolved(&rule.path), access: rule.access };
        if own.contains(&rule.path) {
            split.push(rule);
        } else if program.contains(&rule.path)
            && private.iter().any(|root| rule.path.starts_with(root))
            && super::program_reopenable(&rule.path, &private)
        {
            // Its program inside the private dirs (desktop builds live in
            // `<OctoSense home>/build`): read and execute only, never write.
            split.push(Rule { path: rule.path, access: rule.access & rx });
        } else {
            if program.contains(&rule.path) && private.iter().any(|root| rule.path.starts_with(root)) {
                makepad_widgets::log!("sandbox {}: program path {} holds private data; not reopened", policy.app, rule.path.display());
            }
            around_private(rule, &private, &mut split);
        }
    }
    split
}

/// `rule`, minus the private directories: dropped when it lies inside one;
/// when it contains one, replaced by a rule per entry of its directory,
/// recursing toward the private ones (which get nothing).
pub fn around_private(rule: Rule, private: &[PathBuf], out: &mut Vec<Rule>) {
    if private.iter().any(|p| rule.path.starts_with(p)) {
        return;
    }
    if !private.iter().any(|p| p.starts_with(&rule.path)) {
        out.push(rule);
        return;
    }
    let Ok(entries) = std::fs::read_dir(&rule.path) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        // Landlock grants what a link resolves to: a link into (or above) a
        // private directory gets nothing.
        let real = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if real != path && private.iter().any(|p| real.starts_with(p) || p.starts_with(&real)) {
            continue;
        }
        let access = if path.is_dir() { rule.access } else { rule.access & FILE_RIGHTS };
        around_private(Rule { path, access }, private, out);
    }
}

/// The seccomp program for `processes` on this architecture, `None` where
/// the table below has no numbers for it.
pub fn seccomp_filter(processes: bool) -> Option<Vec<libc::sock_filter>> {
    #[cfg(target_arch = "x86_64")]
    let (arch, denied, forks, clone, clone3): (u32, &[u32], &[u32], u32, u32) = (
        0xC000_003E,
        &[101, 310, 311, 298, 246, 321, 323, 165, 166, 155, 250, 248, 249, 272, 308],
        &[57, 58],
        56,
        435,
    );
    #[cfg(target_arch = "aarch64")]
    let (arch, denied, forks, clone, clone3): (u32, &[u32], &[u32], u32, u32) = (
        0xC000_00B7,
        &[117, 270, 271, 241, 104, 280, 282, 40, 39, 41, 219, 217, 218, 97, 268],
        &[],
        220,
        435,
    );
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = processes;
        return None;
    }
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        const LD_W_ABS: u16 = 0x20;
        const JEQ_K: u16 = 0x15;
        const JSET_K: u16 = 0x45;
        const RET_K: u16 = 0x06;
        const ALLOW: u32 = 0x7fff_0000;
        const ERRNO: u32 = 0x0005_0000;
        const CLONE_THREAD: u32 = 0x0001_0000;
        let st = |code: u16, jt: u8, jf: u8, k: u32| libc::sock_filter { code, jt, jf, k };
        let eperm = ERRNO | libc::EPERM as u32;
        let enosys = ERRNO | libc::ENOSYS as u32;
        let mut f = vec![
            st(LD_W_ABS, 0, 0, 4), // arch
            st(JEQ_K, 1, 0, arch),
            st(RET_K, 0, 0, ALLOW), // an unexpected arch: not ours to judge
            st(LD_W_ABS, 0, 0, 0),  // nr
        ];
        let deny = |f: &mut Vec<libc::sock_filter>, nr: u32, ret: u32| {
            f.push(st(JEQ_K, 0, 1, nr));
            f.push(st(RET_K, 0, 0, ret));
        };
        for &nr in denied {
            deny(&mut f, nr, eperm);
        }
        if !processes {
            for &nr in forks {
                deny(&mut f, nr, eperm);
            }
            deny(&mut f, clone3, enosys);
            // clone: a thread (CLONE_THREAD) passes, a new process does not.
            f.push(st(JEQ_K, 0, 4, clone));
            f.push(st(LD_W_ABS, 0, 0, 16)); // args[0], low word
            f.push(st(JSET_K, 1, 0, CLONE_THREAD));
            f.push(st(RET_K, 0, 0, eperm));
            f.push(st(RET_K, 0, 0, ALLOW));
        }
        f.push(st(RET_K, 0, 0, ALLOW));
        Some(f)
    }
}

/// Everything a sandboxed start installs, prepared before it (between fork
/// and exec the child only opens paths and makes system calls; the runner
/// reads it from a file).
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The kernel's Landlock ABI (0: none).
    pub abi: u32,
    /// Path rules, existing paths only.
    pub rules: Vec<(CString, u64)>,
    /// Restrict TCP to the hub port (`network: none`, ABI 4 and later).
    pub net: bool,
    pub hub_port: u16,
    pub processes: bool,
    /// Variables to remove before the app starts (the runner's; cargo hands
    /// the checkout's `[env]` back).
    pub unset: Vec<String>,
}

impl Plan {
    /// The plan for `policy` on this kernel.
    pub fn new(policy: &Policy) -> Plan {
        let abi = landlock_abi();
        let rules = if abi == 0 {
            Vec::new()
        } else {
            rules(policy, abi)
                .into_iter()
                .filter(|r| r.path.exists())
                .filter_map(|r| CString::new(r.path.as_os_str().as_bytes()).ok().map(|c| (c, r.access)))
                .collect()
        };
        Plan {
            abi,
            rules,
            net: abi >= 4 && policy.network == Network::None,
            hub_port: policy.hub_port,
            processes: policy.processes,
            unset: policy.cargo_env_unset.clone(),
        }
    }

    /// One line per field; paths hex-encoded (they may hold any byte).
    pub fn to_text(&self) -> String {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        let mut out = format!("octosense-sandbox-plan 1\nabi {}\nnet {}\nport {}\nprocesses {}\n", self.abi, self.net as u8, self.hub_port, self.processes as u8);
        for name in &self.unset {
            out.push_str(&format!("unset {}\n", hex(name.as_bytes())));
        }
        for (path, access) in &self.rules {
            out.push_str(&format!("rule {access:x} {}\n", hex(path.as_bytes())));
        }
        out
    }

    /// The plan [`Self::to_text`] wrote; `None` for anything else.
    pub fn from_text(text: &str) -> Option<Plan> {
        let unhex = |s: &str| -> Option<Vec<u8>> {
            (0..s.len()).step_by(2).map(|i| s.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect()
        };
        let mut lines = text.lines();
        if lines.next()? != "octosense-sandbox-plan 1" {
            return None;
        }
        let mut plan = Plan { abi: 0, rules: Vec::new(), net: false, hub_port: 0, processes: false, unset: Vec::new() };
        for line in lines {
            let (key, rest) = line.split_once(' ')?;
            match key {
                "abi" => plan.abi = rest.parse().ok()?,
                "net" => plan.net = rest == "1",
                "port" => plan.hub_port = rest.parse().ok()?,
                "processes" => plan.processes = rest == "1",
                "unset" => plan.unset.push(String::from_utf8(unhex(rest)?).ok()?),
                "rule" => {
                    let (access, path) = rest.split_once(' ')?;
                    plan.rules.push((CString::new(unhex(path)?).ok()?, u64::from_str_radix(access, 16).ok()?));
                }
                _ => return None,
            }
        }
        Some(plan)
    }

    /// What [`Applied`] says about the layers.
    fn layers(&self, network: Network, filter: bool) -> Vec<String> {
        let mut layers = Vec::new();
        if self.abi > 0 {
            layers.push(format!("landlock abi {}{}", self.abi, if self.net { " + ports" } else { "" }));
        } else {
            layers.push("no landlock (kernel lacks it): paths are not restricted".to_string());
        }
        if self.abi > 0 && self.abi < 4 && network == Network::None {
            layers.push("network not restricted (landlock < 4)".into());
        }
        layers.push(if filter { "seccomp".into() } else { "no seccomp on this architecture".to_string() });
        layers
    }

    /// Install it on the calling thread (then exec): no_new_privs, the
    /// Landlock ruleset, the seccomp `filter`. Best-effort: a layer the
    /// kernel refuses is skipped.
    ///
    /// # Safety
    /// Only system calls and no allocation: callable between fork and exec.
    unsafe fn install(&self, filter: Option<&[libc::sock_filter]>) {
        unsafe {
            // Landlock and seccomp both need no_new_privs.
            libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
            if self.abi > 0 {
                let attr = RulesetAttr {
                    handled_access_fs: handled_fs(self.abi),
                    handled_access_net: if self.net { NET_BIND_TCP | NET_CONNECT_TCP } else { 0 },
                };
                let size = if self.abi >= 4 { std::mem::size_of::<RulesetAttr>() } else { 8 };
                let ruleset = libc::syscall(SYS_LANDLOCK_CREATE_RULESET, &attr as *const RulesetAttr, size, 0u32) as i32;
                if ruleset >= 0 {
                    for (path, access) in &self.rules {
                        let fd = libc::open(path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC);
                        if fd < 0 {
                            continue;
                        }
                        let rule = PathBeneathAttr { allowed_access: *access, parent_fd: fd };
                        libc::syscall(SYS_LANDLOCK_ADD_RULE, ruleset, LANDLOCK_RULE_PATH_BENEATH, &rule as *const PathBeneathAttr, 0u32);
                        libc::close(fd);
                    }
                    if self.net {
                        let rule = NetPortAttr { allowed_access: NET_CONNECT_TCP, port: self.hub_port as u64 };
                        libc::syscall(SYS_LANDLOCK_ADD_RULE, ruleset, LANDLOCK_RULE_NET_PORT, &rule as *const NetPortAttr, 0u32);
                    }
                    libc::syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset, 0u32);
                    libc::close(ruleset);
                }
            }
            if let Some(filter) = filter {
                let prog = libc::sock_fprog { len: filter.len() as u16, filter: filter.as_ptr() as *mut libc::sock_filter };
                libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &prog as *const libc::sock_fprog);
            }
        }
    }
}

/// The argument that puts the shell's binary in runner mode:
/// `<shell> --octosense-sandbox-runner <plan file> <app> [args...]`.
pub const RUNNER_FLAG: &str = "--octosense-sandbox-runner";

static RUNNER_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The cargo `runner` variable for this host's target triple.
pub fn runner_var() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER"
    } else {
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER"
    }
}

/// Before `main` (an `.init_array` entry, [`runner_entry!`]): marks this
/// binary as able to run sandboxed apps and, in runner mode, becomes the
/// app. Reads its arguments from `/proc/self/cmdline`, so it does not rely
/// on the C library passing them to init functions.
pub fn runner_hook() {
    RUNNER_READY.store(true, std::sync::atomic::Ordering::Relaxed);
    let Ok(cmdline) = std::fs::read("/proc/self/cmdline") else { return };
    let mut args: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
    if args.last().is_some_and(|a| a.is_empty()) {
        args.pop();
    }
    if args.get(1).copied() != Some(RUNNER_FLAG.as_bytes()) {
        return;
    }
    let error = match (args.get(2), args.get(3)) {
        (Some(plan), Some(_)) => run_as_runner(std::ffi::OsStr::from_bytes(plan).as_ref(), &args[3..]),
        _ => "usage: <shell> --octosense-sandbox-runner <plan> <program> [args...]".to_string(),
    };
    eprintln!("octosense sandbox runner: {error}; the app did not start");
    std::process::exit(126);
}

/// Install the plan at `plan` on this process and exec `argv`; returns only
/// on failure, with why.
pub fn run_as_runner(plan: &std::path::Path, argv: &[&[u8]]) -> String {
    let text = match std::fs::read_to_string(plan) {
        Ok(t) => t,
        Err(e) => return format!("read {}: {e}", plan.display()),
    };
    let _ = std::fs::remove_file(plan);
    let Some(plan) = Plan::from_text(&text) else { return "not a sandbox plan".into() };
    let Ok(argv): Result<Vec<CString>, _> = argv.iter().map(|a| CString::new(a.to_vec())).collect() else {
        return "an argument holds a NUL".into();
    };
    for name in &plan.unset {
        // Single-threaded, before main: nothing else reads the environment.
        std::env::remove_var(name);
    }
    let filter = seccomp_filter(plan.processes);
    let mut ptrs: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    unsafe {
        plan.install(filter.as_deref());
        libc::execv(ptrs[0], ptrs.as_ptr());
    }
    format!("exec {}: {}", argv[0].to_string_lossy(), std::io::Error::last_os_error())
}

/// Where a runner's plan is written for a launch through cargo.
fn write_plan(policy: &Policy, plan: &Plan) -> Result<PathBuf, String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("octosense-sandbox-{}-{}-{n}.plan", std::process::id(), policy.app));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    file.write_all(plan.to_text().as_bytes()).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

/// Put Landlock and seccomp on `cmd`: installed in the child before exec,
/// or, `via_cargo`, on the app alone through the shell as cargo's runner.
pub fn apply(cmd: &mut Command, policy: &Policy, via_cargo: bool) -> Applied {
    let plan = Plan::new(policy);
    let filter = seccomp_filter(plan.processes);
    let mut layers = plan.layers(policy.network, filter.is_some());
    if via_cargo {
        if !RUNNER_READY.load(std::sync::atomic::Ordering::Relaxed) {
            return Applied::Unavailable(format!("{}: this binary cannot run a sandboxed app through cargo (no runner entry); running unsandboxed", policy.app));
        }
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => return Applied::Unavailable(format!("{}: {e}", policy.app)),
        };
        let file = match write_plan(policy, &plan) {
            Ok(file) => file,
            Err(e) => return Applied::Unavailable(format!("{}: {e}", policy.app)),
        };
        // cargo splits the runner on whitespace.
        let runner = format!("{} {RUNNER_FLAG} {}", exe.display(), file.display());
        if exe.to_string_lossy().chars().chain(file.to_string_lossy().chars()).any(char::is_whitespace) {
            let _ = std::fs::remove_file(&file);
            return Applied::Unavailable(format!("{}: a path of the runner has whitespace ({runner})", policy.app));
        }
        cmd.env(runner_var(), runner);
        layers.push("launched through cargo: the build runs outside, the app through the shell as cargo's runner".into());
    } else {
        unsafe {
            cmd.pre_exec(move || {
                plan.install(filter.as_deref());
                Ok(())
            });
        }
    }
    Applied::Sandboxed(format!("{} ({})", policy.summary(), layers.join(", ")))
}
