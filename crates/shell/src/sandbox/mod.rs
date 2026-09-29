//! Process sandboxes for native apps hosted in their own process (ADR 0004
//! §3 and the §11 enforcement table: "the OS sandbox allows the jail, its
//! secrets and `external` only").
//!
//! [`Policy::for_app`] turns a `native-apps.json` entry (its `sandbox` and
//! `storage` blocks) into one [`Policy`]; [`command`] builds the child's
//! [`Command`] where `clients::spawn_client` starts it. The same policy is
//! built for every app; only its manifest entry differs (the Terminal's is
//! necessarily broad: `home:rw`, processes and any network).
//!
//! | OS | Mechanism | Status |
//! | --- | --- | --- |
//! | macOS | a Seatbelt profile ([`macos`]) run through `/usr/bin/sandbox-exec`; a `cargo run` launch gets it as cargo's `runner`, so only the app is sandboxed, never the build | built and tested |
//! | Linux | Landlock for paths and TCP ports, seccomp for ptrace and friends ([`linux`]), installed between fork and exec; best-effort, logged per layer when the kernel lacks it | built, compile-checked; not run here |
//! | Windows | AppContainer (design below) | **TODO**: not built; a process app runs unsandboxed and the shell says so in its log |
//!
//! **Files.** The person's data roots ([`Policy::protected`]: the home
//! directory and mounted volumes) are closed except the app's jail
//! (`<octosense home>/apps/<id>/`), its secrets (`secrets/<id>/`) and its
//! reviewed `external` grants. The system's own read-only locations stay
//! readable: a GPU app needs its libraries, fonts, shader caches and the
//! window server. The app's program and resources (its binary, the
//! checkout it was built from, cargo's source cache for crate resources)
//! are readable, never writable.
//!
//! **Network.** `none`: nothing but the shell's hub on loopback (the socket
//! the app is hosted over). `any`: unrestricted.
//!
//! **Child processes.** `processes: false`: no fork and no exec after the
//! app's own start.
//!
//! **Environment.** Whatever the policy, a process app never inherits the
//! kernel's descriptors or tokens: [`scrub_env`] removes every `OCTOS_*`
//! and `OCTOSENSE_*` variable (host and external tokens, the core and
//! kernel directories, the secrets vault) before the child starts, and on
//! macOS a `cargo run` launch drops the ones the checkout's
//! `.cargo/config.toml` `[env]` would hand back (build paths) in its
//! runner. A process app reaches its agent only over the peer link
//! (`peer_link`).
//!
//! **macOS deprecation.** `sandbox-exec` and `sandbox_init` are marked
//! deprecated in Apple's headers (since 10.8) but remain the mechanism the
//! system and major browsers use for helper processes; the supported
//! replacement, App Sandbox entitlements, applies to a whole signed bundle
//! and cannot express a per-app jail under the person's home. When Apple
//! removes it, a process app falls back to running unsandboxed with a
//! logged warning ([`Applied::Unavailable`]), never to failing to start.
//!
//! **Windows design (TODO).** Create one AppContainer profile per app
//! (`CreateAppContainerProfile("OctoSense.<id>")`), grant its SID
//! `FILE_ALL_ACCESS` on the jail and secrets folders and the `external`
//! grants (ACLs, set once when the folders are created), add the
//! `internetClient` capability only for `network: any` (loopback to the hub
//! needs a loopback exemption for the container, `NetworkIsolation...`),
//! start the child with `PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES`
//! through `CreateProcessW` (std's `Command` cannot pass attribute lists, so
//! this needs its own spawn path), and put it in a job object with
//! `JOB_OBJECT_LIMIT_ACTIVE_PROCESS = 1` for `processes: false`. D3D11
//! shared handles work from an AppContainer. Until then [`command`] reports
//! [`Applied::Unavailable`] on Windows.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::native_apps::{NativeApp, Network};

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(any(target_os = "macos", test))]
pub mod macos;

/// Read-only or read-write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    ReadWrite,
}

/// One app's sandbox, built from its manifest entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Policy {
    pub app: String,
    /// The app's jail, read-write.
    pub jail: PathBuf,
    /// Its secrets folder, read-write (never inside the jail).
    pub secrets: PathBuf,
    /// Its reviewed `external` grants.
    pub external: Vec<(PathBuf, Access)>,
    /// Its program and resources: readable, never writable.
    pub program: Vec<PathBuf>,
    /// The person's data roots: closed except for the grants above.
    pub protected: Vec<PathBuf>,
    /// The host's own private directories (the OctoSense home, the apps
    /// and secrets roots, the kernel's core dir): closed LAST, after every
    /// grant, so even a broad grant (the Terminal's `home:rw`) never reaches
    /// peer host tokens, other apps' jails and secrets, or kernel data. Only
    /// the app's own jail and secrets are opened again inside them.
    pub private: Vec<PathBuf>,
    pub network: Network,
    /// The shell's hub port, reachable on loopback whatever `network` is.
    pub hub_port: u16,
    pub processes: bool,
    /// Host variables a `cargo run` launch would put back from the
    /// checkout's `.cargo/config.toml` `[env]` (build paths such as
    /// `OCTOSENSE_WORKSPACE`): removed again before the app starts where the
    /// platform runs it through a runner ([`cargo_env_host_vars`]).
    pub cargo_env_unset: Vec<String>,
}

/// What [`command`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Applied {
    /// The command now starts the app inside the sandbox.
    Sandboxed(String),
    /// No sandbox on this platform or kernel; the app runs without one.
    Unavailable(String),
}

/// Where a manifest `external` root is, for the person's home `home`.
fn external_root(root: &str, home: &Path) -> Option<PathBuf> {
    Some(match root {
        "home" => home.to_path_buf(),
        "documents" => home.join("Documents"),
        "downloads" => home.join("Downloads"),
        "desktop" => home.join("Desktop"),
        "pictures" => home.join("Pictures"),
        "music" => home.join("Music"),
        "movies" => home.join("Movies"),
        "tmp" => std::env::temp_dir(),
        _ => return None,
    })
}

/// `<root>[/<path>]:ro|rw` (checked by tools/native_apps.py) as a path.
pub fn parse_external(grant: &str, home: &Path) -> Option<(PathBuf, Access)> {
    let (place, access) = grant.rsplit_once(':')?;
    let access = match access {
        "ro" => Access::Read,
        "rw" => Access::ReadWrite,
        _ => return None,
    };
    let (root, rest) = place.split_once('/').unwrap_or((place, ""));
    if rest.split('/').any(|part| part == "..") {
        return None;
    }
    let mut path = external_root(root, home)?;
    if !rest.is_empty() {
        path = path.join(rest);
    }
    Some((path, access))
}

/// The person's home directory.
pub fn person_home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).filter(|h| !h.is_empty()).map(PathBuf::from)
}

impl Policy {
    /// The policy of `app` for a launch whose program and resources live
    /// under `program`, hosted on `hub_port`. `home` is the person's home
    /// (the protected root external grants are relative to); the jail and
    /// secrets come from the host's storage layout.
    pub fn for_app(app: &NativeApp, jail: PathBuf, secrets: PathBuf, home: &Path, program: Vec<PathBuf>, hub_port: u16) -> Policy {
        let external = app.external.iter().filter_map(|grant| parse_external(grant, home)).collect();
        let mut protected = vec![home.to_path_buf()];
        if cfg!(target_os = "macos") {
            protected.push(PathBuf::from("/Volumes"));
        } else if cfg!(target_os = "linux") {
            protected.push(PathBuf::from("/media"));
            protected.push(PathBuf::from("/mnt"));
        }
        Policy {
            app: app.id.to_string(),
            jail,
            secrets,
            external,
            program,
            protected,
            private: Vec::new(),
            network: app.network,
            hub_port,
            processes: app.processes,
            cargo_env_unset: Vec::new(),
        }
    }

    /// The same policy with the dev narrowing of `OCTOSENSE_SANDBOX_NARROW`
    /// (a comma list of app ids): its jail only (no `external`) and no
    /// network; child processes as the manifest says. It can only take
    /// rights away; a test run uses it to see a broad app (the Terminal,
    /// whose shell must still start) refused outside its jail.
    pub fn narrowed(mut self) -> Policy {
        self.external.clear();
        self.network = Network::None;
        self
    }

    /// One line for the shell's log.
    pub fn summary(&self) -> String {
        let external: Vec<String> = self
            .external
            .iter()
            .map(|(p, a)| format!("{}:{}", p.display(), if *a == Access::Read { "ro" } else { "rw" }))
            .collect();
        format!(
            "{}: files = jail + secrets{}{}, network = {}, processes = {}",
            self.app,
            if external.is_empty() { "" } else { " + " },
            external.join(" "),
            match self.network {
                Network::None => format!("hub only (127.0.0.1:{})", self.hub_port),
                Network::Any => "any".into(),
            },
            if self.processes { "allowed" } else { "none" },
        )
    }
}

/// The host's private directories for a sandbox: the OctoSense home, the
/// apps and secrets roots (they may live elsewhere), the kernel's core dir,
/// and the kernel home around a `.octos` core dir. Each resolved, without
/// duplicates.
pub fn host_private_dirs(octosense_home: &Path, apps_root: &Path, secrets_root: &Path, core_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: &Path| {
        if p.as_os_str().is_empty() || p.parent().is_none() {
            return; // never the file system root
        }
        let p = p.to_path_buf();
        if !out.contains(&p) {
            out.push(p);
        }
    };
    push(octosense_home);
    push(apps_root);
    push(secrets_root);
    if let Some(core) = core_dir {
        push(core);
        if core.file_name().is_some_and(|n| n == ".octos") {
            if let Some(kernel_home) = core.parent() {
                push(kernel_home);
            }
        }
    }
    out
}

/// Whether `OCTOSENSE_SANDBOX_NARROW` names `app` (see [`Policy::narrowed`]).
pub fn narrowed_by_env(app: &str) -> bool {
    std::env::var("OCTOSENSE_SANDBOX_NARROW").map(|v| v.split(',').any(|a| a.trim() == app)).unwrap_or(false)
}

/// The environment variables no process app may inherit.
pub fn is_host_secret_var(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.starts_with("OCTOS_") || upper.starts_with("OCTOSENSE_")
}

/// The host variables (`OCTOS_*`, `OCTOSENSE_*`) a checkout's
/// `.cargo/config.toml` `[env]` sets, which `cargo run` hands the app.
pub fn cargo_env_host_vars(config: &str) -> Vec<String> {
    let mut in_env = false;
    let mut out = Vec::new();
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_env = line == "[env]";
            continue;
        }
        if !in_env {
            continue;
        }
        if let Some((name, _)) = line.split_once('=') {
            let name = name.trim();
            if is_host_secret_var(name) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// Remove every host variable from what `cmd` passes on (ADR 0004 §3: a
/// process app never connects to the kernel and never sees the host token).
pub fn scrub_env(cmd: &mut Command) {
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(is_host_secret_var) {
            cmd.env_remove(&name);
        }
    }
    let explicit: Vec<std::ffi::OsString> =
        cmd.get_envs().filter(|(k, v)| v.is_some() && k.to_str().is_some_and(is_host_secret_var)).map(|(k, _)| k.to_os_string()).collect();
    for name in explicit {
        cmd.env_remove(name);
    }
}

/// The command that starts `program args` under `policy` (`None`: no
/// sandbox, the plain command). `via_cargo`: `program` is cargo and the app
/// is what `cargo run` runs at the end, so only the app is sandboxed where
/// the platform allows (macOS, through cargo's `runner`).
pub fn command(program: &Path, args: &[String], policy: Option<&Policy>, via_cargo: bool) -> (Command, Option<Applied>) {
    let Some(policy) = policy else {
        let mut cmd = Command::new(program);
        cmd.args(args);
        return (cmd, None);
    };
    platform_command(program, args, policy, via_cargo)
}

#[cfg(target_os = "macos")]
fn platform_command(program: &Path, args: &[String], policy: &Policy, via_cargo: bool) -> (Command, Option<Applied>) {
    let plain = || {
        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd
    };
    if !Path::new(macos::SANDBOX_EXEC).is_file() {
        return (plain(), Some(Applied::Unavailable(format!("{}: {} is missing", policy.app, macos::SANDBOX_EXEC))));
    }
    let profile = match macos::write_profile(policy) {
        Ok(p) => p,
        Err(e) => return (plain(), Some(Applied::Unavailable(format!("{}: {e}", policy.app)))),
    };
    let how = if via_cargo {
        let mut cmd = plain();
        // cargo re-adds its `[env]` to the app: `env -u` takes the host's
        // back out, outside the sandbox, before sandbox-exec starts the app.
        let unset: String = policy.cargo_env_unset.iter().map(|v| format!("-u {v} ")).collect();
        let runner = if unset.is_empty() {
            format!("{} -f {}", macos::SANDBOX_EXEC, profile.display())
        } else {
            format!("/usr/bin/env {unset}{} -f {}", macos::SANDBOX_EXEC, profile.display())
        };
        cmd.env(macos::runner_var(), runner);
        (cmd, "as cargo's runner")
    } else {
        let mut cmd = Command::new(macos::SANDBOX_EXEC);
        cmd.arg("-f").arg(&profile).arg(program).args(args);
        (cmd, "via sandbox-exec")
    };
    (how.0, Some(Applied::Sandboxed(format!("{} ({}, profile {})", policy.summary(), how.1, profile.display()))))
}

#[cfg(target_os = "linux")]
fn platform_command(program: &Path, args: &[String], policy: &Policy, via_cargo: bool) -> (Command, Option<Applied>) {
    let mut cmd = Command::new(program);
    cmd.args(args);
    let applied = linux::apply(&mut cmd, policy, via_cargo);
    (cmd, Some(applied))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_command(program: &Path, args: &[String], policy: &Policy, _via_cargo: bool) -> (Command, Option<Applied>) {
    let mut cmd = Command::new(program);
    cmd.args(args);
    let why = format!("{}: no process sandbox on this platform yet (Windows AppContainer is a TODO, sandbox/mod.rs); running unsandboxed", policy.app);
    (cmd, Some(Applied::Unavailable(why)))
}

/// Every ancestor of `path`, root first (their metadata is readable so the
/// path itself resolves).
pub fn ancestors(path: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = path.ancestors().skip(1).map(Path::to_path_buf).collect();
    out.reverse();
    out
}

/// A path with symlinks resolved as far as it exists (macOS: `/tmp` is
/// `/private/tmp`, and a sandbox profile matches resolved paths).
pub fn resolved(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    loop {
        if let Ok(real) = existing.canonicalize() {
            let mut out = real;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests;
