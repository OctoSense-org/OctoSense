//! The sandbox, proved on this machine: a child started through
//! [`command`] under a jail-only policy reads its jail and is refused a
//! file beside it, a network connection other than the hub, and a child
//! process; and no child inherits a host token.

#![cfg_attr(not(unix), allow(unused))]

use super::*;
use std::io::Read;

use std::net::TcpListener;

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("octosense-sandbox-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("apps/probe")).unwrap();
        std::fs::create_dir_all(dir.join("secrets/probe")).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn jail_only(root: &Path, hub_port: u16) -> Policy {
    let app = crate::native_apps::NativeApp {
        id: "probe",
        name: None,
        feature: "app-probe",
        bin: Some("probe"),
        macos: crate::native_apps::Hosting::Process,
        windows: crate::native_apps::Hosting::Process,
        linux: crate::native_apps::Hosting::Process,
        android: crate::native_apps::Hosting::Module,
        ios: crate::native_apps::Hosting::Module,
        ohos: crate::native_apps::Hosting::Module,
        wasm: crate::native_apps::Hosting::Module,
        octos: &[],
        tools: &[],
        network: Network::None,
        processes: false,
        accounts: false,
        external: &[],
        storage: "{}",
        tools_json: "[]",
        generic_tools: &[],
        grants: &[],
        system_tools: &[],
        own_tools: &[],
        calls_per_turn: None,
        calls_per_day: None,
        kernel: crate::native_apps::KernelPort::None,
    };
    // The scratch root stands in for the person's home: closed but for the
    // jail. The probe's tools are its program.
    Policy::for_app(&app, root.join("apps/probe"), root.join("secrets/probe"), root, vec!["/bin".into(), "/usr/bin".into()], hub_port)
}

fn run(program: &str, args: &[&str], policy: &Policy) -> (bool, String) {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (mut cmd, applied) = command(Path::new(program), &args, Some(policy));
    assert!(matches!(applied, Some(Applied::Sandboxed(_))), "{applied:?}");
    let out = cmd.output().expect("the probe starts");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

/// What `nc -v -z` said became of its connect: `Ok` when it connected, the
/// reason it printed when the connect failed, `None` when it said neither
/// (it never ran its connect: `sandbox-exec` refused to start it, say).
/// Apple's nc writes `nc: connectx to … failed: <reason>`, OpenBSD's
/// `nc: connect to … failed: <reason>`.
fn connect_outcome(out: &str) -> Option<Result<(), &str>> {
    for line in out.lines() {
        if line.starts_with("Connection to ") && line.ends_with("succeeded!") {
            return Some(Ok(()));
        }
        if line.starts_with("nc: connect") {
            if let Some((_, why)) = line.split_once(" failed: ") {
                return Some(Err(why.trim()));
            }
        }
    }
    None
}

/// A connect the sandbox itself refused: EPERM from Seatbelt and seccomp,
/// EACCES from Landlock. Every other error is decided past the sandbox's
/// check (an absent listener, the network stack), so it means the sandbox
/// let the connect out.
fn refused_by_sandbox(why: &str) -> bool {
    why == "Operation not permitted" || why == "Permission denied"
}

fn sandbox_works_here() -> bool {
    if cfg!(target_os = "macos") {
        return Path::new(macos::SANDBOX_EXEC).is_file();
    }
    #[cfg(target_os = "linux")]
    return linux::landlock_abi() > 0;
    #[allow(unreachable_code)]
    false
}

#[cfg(unix)]
#[test]
fn a_jail_only_app_reads_its_jail_and_is_refused_everything_else() {
    if !sandbox_works_here() {
        eprintln!("no process sandbox on this machine; skipped");
        return;
    }
    let scratch = Scratch::new("jail");
    let root = &scratch.0;
    std::fs::write(root.join("apps/probe/mine.txt"), "mine").unwrap();
    std::fs::write(root.join("secrets/probe/key"), "its own secret").unwrap();
    std::fs::write(root.join("outside.txt"), "not yours").unwrap();
    std::fs::create_dir_all(root.join("apps/other")).unwrap();
    std::fs::write(root.join("apps/other/theirs.txt"), "another app's").unwrap();
    let hub = TcpListener::bind("127.0.0.1:0").unwrap();
    let other = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_port = hub.local_addr().unwrap().port();
    let other_port = other.local_addr().unwrap().port();
    let policy = jail_only(root, hub_port);

    let cat = |p: PathBuf| run("/bin/cat", &[p.to_str().unwrap()], &policy);
    let (ok, out) = cat(root.join("apps/probe/mine.txt"));
    assert!(ok && out.contains("mine"), "its jail: {out}");
    let (ok, out) = cat(root.join("secrets/probe/key"));
    assert!(ok, "its secrets: {out}");
    let (ok, out) = cat(root.join("outside.txt"));
    assert!(!ok && !out.contains("not yours"), "a file outside the jail is refused: {out}");
    let (ok, out) = cat(root.join("apps/other/theirs.txt"));
    assert!(!ok && !out.contains("another app's"), "another app's jail is refused: {out}");
    let (ok, _) = run("/bin/sh", &["-c", &format!("echo x > {}/apps/probe/written", root.display())], &Policy { processes: true, ..policy.clone() });
    assert!(ok, "it writes in its jail");
    let (ok, out) = run("/bin/sh", &["-c", &format!("echo x > {}/written", root.display())], &Policy { processes: true, ..policy.clone() });
    assert!(!ok, "it writes nothing outside: {out}");

    // Network: the hub on loopback, nothing else. What is asked is the
    // sandbox's verdict on each connect, so nc says what became of it
    // (`-v`) instead of only exiting 1, and only the sandbox's own refusal
    // counts as one: a connect the sandbox lets out may still be refused by
    // the network stack, which is not this test's business.
    let nc = |port: u16| run("/usr/bin/nc", &["-v", "-n", "-z", "-w", "2", "127.0.0.1", &port.to_string()], &policy);
    let (_, out) = nc(hub_port);
    match connect_outcome(&out) {
        Some(Ok(())) => {}
        Some(Err(why)) => {
            assert!(!refused_by_sandbox(why), "the sandbox lets the hub through: {out}");
            eprintln!("the hub's connect left the sandbox and failed past it: {why}");
        }
        None => panic!("nc said nothing about the hub's connect: {out}"),
    }
    let (ok, out) = nc(other_port);
    assert!(!ok && matches!(connect_outcome(&out), Some(Err(why)) if refused_by_sandbox(why)), "any other connection is refused by the sandbox: {out}");

    // Processes: none.
    let (ok, out) = run("/bin/sh", &["-c", "/bin/echo first; /bin/echo spawned"], &policy);
    assert!(!ok || !out.contains("spawned"), "no child process: {out}");
    drop((hub, other));
}

/// A program reached through a link that points outside its roots still
/// starts (found on Ubuntu 26.04, where `/usr/bin/cat` links into
/// `/usr/lib/cargo/bin/coreutils/`): the sandbox allows the file it really
/// runs, and nothing else beside it.
#[cfg(target_os = "linux")]
#[test]
fn a_program_reached_through_a_link_outside_its_roots_starts() {
    if !sandbox_works_here() {
        eprintln!("no process sandbox on this machine; skipped");
        return;
    }
    let scratch = Scratch::new("linkedprog");
    let root = &scratch.0;
    std::fs::write(root.join("apps/probe/mine.txt"), "mine").unwrap();
    std::fs::create_dir_all(root.join("real")).unwrap();
    std::fs::create_dir_all(root.join("links")).unwrap();
    // Named `cat` wherever it is: a multicall coreutils picks by name.
    std::fs::copy(resolved(Path::new("/bin/cat")), root.join("real/cat")).unwrap();
    std::fs::write(root.join("real/beside.txt"), "not the program").unwrap();
    std::os::unix::fs::symlink(root.join("real/cat"), root.join("links/cat")).unwrap();
    let mut policy = jail_only(root, 1);
    policy.program = vec![root.join("links")];
    let tool = root.join("links/cat");
    let (ok, out) = run(tool.to_str().unwrap(), &[root.join("apps/probe/mine.txt").to_str().unwrap()], &policy);
    assert!(ok && out.contains("mine"), "the linked program starts and reads its jail: {out}");
    let (ok, out) = run(tool.to_str().unwrap(), &[root.join("real/beside.txt").to_str().unwrap()], &policy);
    assert!(!ok && !out.contains("not the program"), "only the program file is opened, not its directory: {out}");
}

/// A kernel without Landlock (before 5.13, or with it disabled): the app
/// still starts, seccomp still takes, and the log line says the paths are
/// not restricted. Runs only where Landlock is missing; on a Landlock
/// kernel, run it under a filter that hides it:
///
/// ```sh
/// sudo systemd-run --uid=$USER --pty --wait -p SystemCallErrorNumber=ENOSYS \
///   -p 'SystemCallFilter=~landlock_create_ruleset landlock_add_rule landlock_restrict_self' \
///   <test binary> without_landlock
/// ```
#[cfg(target_os = "linux")]
#[test]
fn without_landlock_the_app_still_starts_under_seccomp_and_says_so() {
    if linux::landlock_abi() > 0 {
        eprintln!("this kernel has Landlock; skipped (see the doc comment to hide it)");
        return;
    }
    let scratch = Scratch::new("nolandlock");
    let root = &scratch.0;
    std::fs::write(root.join("apps/probe/mine.txt"), "mine").unwrap();
    let policy = jail_only(root, 1);
    let args = vec![root.join("apps/probe/mine.txt").to_string_lossy().to_string()];
    let (mut cmd, applied) = command(Path::new("/bin/cat"), &args, Some(&policy));
    let Some(Applied::Sandboxed(how)) = applied else { panic!("{applied:?}") };
    assert!(how.contains("no landlock (kernel lacks it): paths are not restricted") && how.contains("seccomp"), "{how}");
    let out = cmd.output().expect("the app starts without Landlock");
    assert!(out.status.success() && String::from_utf8_lossy(&out.stdout).contains("mine"));
    // seccomp still refuses a child process.
    let (mut cmd, _) = command(Path::new("/bin/sh"), &["-c".into(), "/bin/echo first; /bin/echo spawned".into()], Some(&policy));
    let out = cmd.output().expect("sh starts");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains("spawned"), "no child process without Landlock either: {text}");
}

/// The seccomp program run on one system call, as the kernel would (the
/// five classic-BPF instructions it uses).
#[cfg(target_os = "linux")]
fn seccomp_verdict(filter: &[libc::sock_filter], arch: u32, nr: u32, arg0: u64) -> u32 {
    seccomp_verdict_args(filter, arch, nr, [arg0, 0, 0])
}

#[cfg(target_os = "linux")]
fn seccomp_verdict_args(filter: &[libc::sock_filter], arch: u32, nr: u32, args: [u64; 3]) -> u32 {
    let word = |offset: u32| -> u32 {
        match offset {
            0 => nr,
            4 => arch,
            16 | 24 | 32 => args[(offset as usize - 16) / 8] as u32,
            _ => 0,
        }
    };
    let (mut pc, mut acc) = (0usize, 0u32);
    loop {
        let i = filter[pc];
        match i.code {
            0x20 => acc = word(i.k),
            0x15 => pc += if acc == i.k { i.jt } else { i.jf } as usize,
            0x45 => pc += if acc & i.k != 0 { i.jt } else { i.jf } as usize,
            0x54 => acc &= i.k,
            0x06 => return i.k,
            other => panic!("instruction {other:#x} not modelled"),
        }
        pc += 1;
    }
}

/// Another ABI's system calls never get past the filter (found in review
/// and on a real kernel: an x86_64 app forked through i386's `int 0x80`
/// fork, number 2, under `processes: false`).
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn seccomp_refuses_the_other_abis_of_the_kernel() {
    const X86_64: u32 = 0xC000_003E;
    const I386: u32 = 0x4000_0003;
    const EPERM: u32 = 0x0005_0000 | libc::EPERM as u32;
    const ALLOW: u32 = 0x7fff_0000;
    let filter = linux::seccomp_filter(false, false).unwrap();
    assert_eq!(seccomp_verdict(&filter, X86_64, 57, 0), EPERM, "fork");
    assert_eq!(seccomp_verdict(&filter, X86_64, 0, 0), ALLOW, "read");
    assert_eq!(seccomp_verdict(&filter, X86_64, 56, 0x0001_0000), ALLOW, "a thread's clone");
    assert_eq!(seccomp_verdict(&filter, X86_64, 56, 0), EPERM, "a process's clone");
    assert_eq!(seccomp_verdict(&filter, I386, 2, 0), EPERM, "i386 fork");
    assert_eq!(seccomp_verdict(&filter, I386, 3, 0), EPERM, "any i386 call");
    assert_eq!(seccomp_verdict(&filter, X86_64, 0x4000_0000 | 57, 0), EPERM, "x32 fork");
    assert_eq!(seccomp_verdict(&filter, X86_64, 0x4000_0000, 0), EPERM, "any x32 call");
    let broad = linux::seccomp_filter(true, false).unwrap();
    assert_eq!(seccomp_verdict(&broad, X86_64, 57, 0), ALLOW, "fork with processes: true");
    assert_eq!(seccomp_verdict(&broad, I386, 2, 0), EPERM, "never another ABI");
    assert_eq!(seccomp_verdict(&broad, X86_64, 101, 0), EPERM, "ptrace, always");
}

/// Under `home:rw`, what the next build reads or runs stays read-only on
/// Linux too (macOS's profile does it after every grant): `~/.cargo`,
/// `~/.rustup`, the checkout and its target dir, a `rust-toolchain` file,
/// the shell's own directory. The rest of the home stays writable.
#[cfg(target_os = "linux")]
#[test]
fn home_rw_leaves_the_next_builds_inputs_read_only() {
    let scratch = Scratch::new("readonly");
    let root = &scratch.0;
    for dir in [".cargo/bin", ".rustup", "src/app/target/release", "Documents", "bin/shell", ".octosense/apps/probe", ".octosense/secrets/probe"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("src/rust-toolchain.toml"), "[toolchain]\n").unwrap();
    std::fs::write(root.join("Documents/notes.txt"), "mine").unwrap();
    let mut p = home_rw_with_octosense_home(root);
    let read_only = [".cargo", ".rustup", "src/app", "src/rust-toolchain.toml", "bin/shell"].map(|r| root.join(r));
    p.read_only = read_only.to_vec();
    let rules = linux::rules(&p, 3);
    let rx = linux::read_exec();
    for ro in &read_only {
        let ro = resolved(ro);
        let covering: Vec<_> = rules.iter().filter(|r| ro.starts_with(&r.path) || r.path.starts_with(&ro)).collect();
        assert!(covering.iter().any(|r| r.path == ro && r.access & (1 << 2) != 0), "{} stays readable: {covering:?}", ro.display());
        assert!(covering.iter().all(|r| r.access & !rx == 0), "{} is never writable: {covering:?}", ro.display());
    }
    let documents = resolved(&root.join("Documents"));
    assert!(rules.iter().any(|r| r.path == documents && r.access & (1 << 1) != 0), "the rest of the home stays writable");
    if !sandbox_works_here() {
        return;
    }
    let sh = |script: String| run("/bin/sh", &["-c", &script], &Policy { processes: true, ..p.clone() });
    let (ok, out) = sh(format!("echo x > {}/.cargo/bin/cargo", root.display()));
    assert!(!ok, "no write into ~/.cargo: {out}");
    let (ok, out) = sh(format!("echo x > {}/src/app/target/release/app", root.display()));
    assert!(!ok, "no write into the target dir: {out}");
    let (ok, out) = sh(format!("cat {0}/src/rust-toolchain.toml && echo x >> {0}/Documents/notes.txt", root.display()));
    assert!(ok, "reads a build input, writes elsewhere in the home: {out}");
}

/// `network: none` leaves plain TCP (Landlock's port rule governs it) and
/// local sockets, and refuses every other family (found on a real kernel:
/// a jail-only app sent UDP to 1.1.1.1:53; review: vsock, Bluetooth, TIPC
/// and RDS were open too).
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn seccomp_keeps_network_none_to_tcp_and_local_sockets() {
    const X86_64: u32 = 0xC000_003E;
    const SOCKET: u32 = 41;
    const EPERM: u32 = 0x0005_0000 | libc::EPERM as u32;
    const ALLOW: u32 = 0x7fff_0000;
    let (inet, inet6, unix, netlink) = (libc::AF_INET as u64, libc::AF_INET6 as u64, libc::AF_UNIX as u64, libc::AF_NETLINK as u64);
    let (stream, dgram, raw) = (libc::SOCK_STREAM as u64, libc::SOCK_DGRAM as u64, libc::SOCK_RAW as u64);
    let flags = (libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK) as u64;
    let none = linux::seccomp_filter(true, true).unwrap();
    let verdict = |family, kind, protocol| seccomp_verdict_args(&none, X86_64, SOCKET, [family, kind, protocol]);
    assert_eq!(verdict(inet, stream, 0), ALLOW, "TCP");
    assert_eq!(verdict(inet6, stream | flags, libc::IPPROTO_TCP as u64), ALLOW, "TCP over IPv6 with flags");
    assert_eq!(verdict(inet, dgram, 0), EPERM, "UDP");
    assert_eq!(verdict(inet6, dgram | flags, 0), EPERM, "UDP over IPv6");
    assert_eq!(verdict(inet, raw, 1), EPERM, "raw");
    assert_eq!(verdict(inet, stream, libc::IPPROTO_SCTP as u64), EPERM, "SCTP stream");
    assert_eq!(verdict(inet, stream, 262), EPERM, "MPTCP");
    assert_eq!(verdict(unix, dgram, 0), ALLOW, "Unix");
    assert_eq!(verdict(unix, stream | flags, 0), ALLOW, "Unix stream");
    assert_eq!(verdict(netlink, raw, 0), ALLOW, "netlink");
    for (family, name) in [
        (libc::AF_VSOCK, "vsock"),
        (libc::AF_BLUETOOTH, "Bluetooth"),
        (libc::AF_TIPC, "TIPC"),
        (libc::AF_RDS, "RDS"),
        (libc::AF_PACKET, "packet"),
        (libc::AF_CAN, "CAN"),
        (libc::AF_ALG, "kernel crypto"),
        (libc::AF_XDP, "XDP"),
    ] {
        assert_eq!(verdict(family as u64, stream, 0), EPERM, "{name}");
    }
    assert_eq!(seccomp_verdict(&none, X86_64, 425, 0), EPERM, "io_uring");
    assert_eq!(seccomp_verdict(&none, X86_64, 57, 0), ALLOW, "fork, processes: true");
    let any = linux::seccomp_filter(true, false).unwrap();
    assert_eq!(seccomp_verdict_args(&any, X86_64, SOCKET, [inet, dgram, 0]), ALLOW, "UDP with network: any");
    assert_eq!(seccomp_verdict_args(&any, X86_64, SOCKET, [libc::AF_VSOCK as u64, stream, 0]), ALLOW, "vsock with network: any");
}

#[test]
fn the_terminals_manifest_gives_a_broad_sandbox_and_narrowing_only_takes_away() {
    let terminal = crate::native_apps::find("terminal").unwrap();
    let home = Path::new("/home/person");
    let p = Policy::for_app(terminal, "/h/apps/terminal".into(), "/h/secrets/terminal".into(), home, vec![], 8765);
    assert_eq!(p.external, vec![(home.to_path_buf(), Access::ReadWrite)]);
    assert_eq!((p.network, p.processes), (Network::Any, true));
    let n = p.clone().narrowed();
    assert!(n.external.is_empty());
    assert_eq!((n.network, n.processes), (Network::None, true), "narrowing keeps the shell's processes");
    assert_eq!((n.jail, n.secrets), (p.jail, p.secrets), "narrowing never moves the jail");
    let reference = crate::native_apps::find("reference").unwrap();
    let r = Policy::for_app(reference, "/h/apps/reference".into(), "/h/secrets/reference".into(), home, vec![], 8765);
    assert!(r.external.is_empty());
    assert_eq!((r.network, r.processes), (Network::None, false));
}

#[test]
fn external_grants_parse_under_their_roots_only() {
    let home = Path::new("/home/person");
    assert_eq!(parse_external("home:rw", home), Some((home.to_path_buf(), Access::ReadWrite)));
    assert_eq!(parse_external("documents/Notes:ro", home), Some((home.join("Documents/Notes"), Access::Read)));
    assert_eq!(parse_external("home/../etc:ro", home), None);
    assert_eq!(parse_external("/etc:ro", home), None);
    assert_eq!(parse_external("home:rwx", home), None);
}

#[test]
fn the_macos_profile_closes_the_roots_then_opens_the_grants() {
    let p = jail_only(Path::new("/nonexistent/home"), 8765);
    let text = macos::profile(&p);
    let deny = text.find("(deny file-read* file-write* (subpath \"/nonexistent/home\")").expect(&text);
    let jail = text.find("(allow file-read* file-write* (subpath \"/nonexistent/home/apps/probe\")").expect(&text);
    assert!(deny < jail, "the last matching rule wins: grants come after the close\n{text}");
    assert!(text.contains("(allow file-read-metadata (literal \"/nonexistent/home\") (literal \"/nonexistent/home/apps\")"), "{text}");
    assert!(text.contains("(allow network-outbound (remote ip \"localhost:8765\"))"), "{text}");
    assert!(text.contains("(deny process-fork)"), "{text}");
    let broad = Policy { network: Network::Any, processes: true, ..p };
    let text = macos::profile(&broad);
    assert!(!text.contains("network") && !text.contains("process-"), "{text}");
}

#[test]
fn cargos_env_block_host_variables_are_taken_back_out() {
    let config = "[env]\nMAKEPAD_BUNDLE_NAME = { value = \"OctoSense\" }\nOCTOSENSE_WORKSPACE = { value = \".sources\", relative = true }\nOCTOS_X = \"1\"\n[target.x]\nOCTOSENSE_NOT_ENV = 1\n";
    assert_eq!(cargo_env_host_vars(config), vec!["OCTOSENSE_WORKSPACE".to_string(), "OCTOS_X".to_string()]);
    // The repository's own config is read the same way.
    let repo = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.cargo/config.toml")).unwrap_or_default();
    for name in cargo_env_host_vars(&repo) {
        assert!(is_host_secret_var(&name));
    }
}

#[cfg(unix)]
#[test]
fn no_child_inherits_a_host_token_or_a_kernel_path() {
    assert!(is_host_secret_var("OCTOS_AUTH_TOKEN"));
    assert!(is_host_secret_var("OCTOS_HOST_EXTERNAL_TOKEN"));
    assert!(is_host_secret_var("OCTOS_APP_CORE_DIR"));
    assert!(is_host_secret_var("OCTOSENSE_SECRETS"));
    assert!(!is_host_secret_var("STUDIO_HOST"));
    assert!(!is_host_secret_var("HOME"));
    let mut cmd = Command::new("/usr/bin/env");
    // As if the shell's own environment held them (the kernel's), plus one
    // set on the command itself.
    cmd.env("OCTOS_AUTH_TOKEN", "host-token")
        .env("OCTOS_APP_CORE_DIR", "/core")
        .env("OCTOSENSE_SECRETS", "/vault")
        .env("STUDIO_HOST", "http://127.0.0.1:8765");
    scrub_env(&mut cmd);
    let mut out = String::new();
    let mut child = cmd.stdout(std::process::Stdio::piped()).spawn().unwrap();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    child.wait().unwrap();
    assert!(out.contains("STUDIO_HOST="), "{out}");
    for line in out.lines() {
        let name = line.split('=').next().unwrap_or("");
        assert!(!is_host_secret_var(name), "the child sees {line}");
    }
}

// ---------------------------------------------------------------- the OctoSense home (G6)

/// A broad app (the Terminal's `home:rw`) whose scratch "home" also holds
/// the OctoSense home and the kernel's core dir, as on a real machine.
fn home_rw_with_octosense_home(root: &Path) -> Policy {
    let octo = root.join(".octosense");
    let jail = octo.join("apps/probe");
    let secrets = octo.join("secrets/probe");
    let mut p = jail_only(root, 1);
    p.jail = jail;
    p.secrets = secrets;
    p.external = vec![(root.to_path_buf(), Access::ReadWrite)];
    p.network = Network::Any;
    p.processes = true;
    p.private = host_private_dirs(&octo, &octo.join("apps"), &octo.join("secrets"), Some(&root.join("octos-home/.octos")));
    p
}

#[cfg(unix)]
#[test]
fn home_rw_never_reaches_the_octosense_home_or_the_kernel() {
    if !sandbox_works_here() {
        eprintln!("no process sandbox on this machine; skipped");
        return;
    }
    let scratch = Scratch::new("octohome");
    let root = &scratch.0;
    let octo = root.join(".octosense");
    for dir in ["apps/probe", "apps/other", "secrets/probe", "secrets/other", "app-peers"] {
        std::fs::create_dir_all(octo.join(dir)).unwrap();
    }
    std::fs::create_dir_all(root.join("octos-home/.octos")).unwrap();
    std::fs::create_dir_all(root.join("Documents")).unwrap();
    std::fs::write(root.join("Documents/note.txt"), "the person's note").unwrap();
    std::fs::write(octo.join("apps/probe/mine.txt"), "mine").unwrap();
    std::fs::write(octo.join("secrets/probe/key"), "its own secret").unwrap();
    std::fs::write(octo.join("apps/other/theirs.txt"), "another app's").unwrap();
    std::fs::write(octo.join("secrets/other/key"), "another app's secret").unwrap();
    std::fs::write(octo.join("app-peers/rinx.token"), "peer-host-token").unwrap();
    std::fs::write(octo.join("settings.json"), "{}").unwrap();
    std::fs::write(root.join("octos-home/.octos/profile.json"), "kernel data").unwrap();
    std::fs::write(root.join("octos-home/notes.md"), "kernel home").unwrap();
    let policy = home_rw_with_octosense_home(root);

    let cat = |p: PathBuf| run("/bin/cat", &[p.to_str().unwrap()], &policy);
    let (ok, out) = cat(root.join("Documents/note.txt"));
    assert!(ok && out.contains("the person's note"), "home:rw still reaches the person's files: {out}");
    let (ok, out) = cat(octo.join("apps/probe/mine.txt"));
    assert!(ok && out.contains("mine"), "its own jail: {out}");
    let (ok, out) = cat(octo.join("secrets/probe/key"));
    assert!(ok && out.contains("its own secret"), "its own secrets: {out}");
    for (path, what) in [
        (octo.join("app-peers/rinx.token"), "peer-host-token"),
        (octo.join("apps/other/theirs.txt"), "another app's"),
        (octo.join("secrets/other/key"), "another app's secret"),
        (octo.join("settings.json"), "{}"),
        (root.join("octos-home/.octos/profile.json"), "kernel data"),
        (root.join("octos-home/notes.md"), "kernel home"),
    ] {
        let (ok, out) = cat(path.clone());
        assert!(!ok && !out.contains(what), "{} must be refused: {out}", path.display());
    }
    let sh = |script: String| run("/bin/sh", &["-c", &script], &policy);
    let (ok, out) = sh(format!("echo x > {}/app-peers/planted.token", octo.display()));
    assert!(!ok, "nothing is written into the OctoSense home: {out}");
    let (ok, out) = sh(format!("ls {}/apps", octo.display()));
    assert!(!ok || !out.contains("other"), "other apps' jails are not even listed: {out}");
    let (ok, out) = sh(format!("echo x > {}/apps/probe/written", octo.display()));
    assert!(ok, "it writes in its own jail: {out}");
}

#[test]
fn the_macos_profile_closes_the_octosense_home_after_every_grant() {
    let root = Path::new("/nonexistent/home");
    let p = home_rw_with_octosense_home(root);
    let text = macos::profile(&p);
    let grant = text.find("(allow file-read* file-write* (subpath \"/nonexistent/home/.octosense/apps/probe\") (subpath \"/nonexistent/home/.octosense/secrets/probe\") (subpath \"/nonexistent/home\"))").expect(&text);
    let deny = text.find("(deny file-read* file-write* (subpath \"/nonexistent/home/.octosense\")").expect(&text);
    let own = text.rfind("(allow file-read* file-write* (subpath \"/nonexistent/home/.octosense/apps/probe\") (subpath \"/nonexistent/home/.octosense/secrets/probe\"))").expect(&text);
    assert!(grant < deny && deny < own, "home:rw, then the private deny, then only its own jail and secrets\n{text}");
    for dir in ["/nonexistent/home/octos-home/.octos", "/nonexistent/home/octos-home"] {
        assert!(text[deny..].contains(&format!("(subpath \"{dir}\")")), "{dir} is closed\n{text}");
    }
    // Nothing private: no extra rules.
    let mut plain = p.clone();
    plain.private.clear();
    assert!(!macos::profile(&plain).contains("private directories"));
}

#[test]
fn the_private_dirs_are_the_homes_roots_and_the_kernels() {
    let octo = Path::new("/h/.octosense");
    let dirs = host_private_dirs(octo, &octo.join("apps"), Path::new("/elsewhere/secrets"), Some(Path::new("/h/octos-home/.octos")));
    assert_eq!(
        dirs,
        vec![octo.to_path_buf(), octo.join("apps"), PathBuf::from("/elsewhere/secrets"), PathBuf::from("/h/octos-home/.octos"), PathBuf::from("/h/octos-home")]
    );
    assert!(host_private_dirs(Path::new("/"), Path::new("/a"), Path::new("/b"), Some(Path::new("/core"))).iter().all(|p| p != Path::new("/")), "never the root");
}

#[cfg(target_os = "linux")]
#[test]
fn landlock_splits_a_grant_around_the_private_dirs() {
    let scratch = Scratch::new("split");
    let root = &scratch.0;
    let octo = root.join(".octosense");
    std::fs::create_dir_all(octo.join("apps/probe")).unwrap();
    std::fs::create_dir_all(root.join("Documents")).unwrap();
    std::fs::write(root.join("top.txt"), "t").unwrap();
    std::os::unix::fs::symlink(&octo, root.join("sneaky")).unwrap();
    let mut out = Vec::new();
    linux::around_private(linux::Rule { path: root.clone(), access: 0xfff }, &[octo.clone()], &mut out);
    let paths: Vec<PathBuf> = out.iter().map(|r| r.path.clone()).collect();
    assert!(paths.contains(&root.join("Documents")) && paths.contains(&root.join("top.txt")), "{paths:?}");
    assert!(!paths.iter().any(|p| p.starts_with(&octo) || p == root), "{paths:?}");
    assert!(!paths.contains(&root.join("sneaky")), "a link into a private dir gets nothing: {paths:?}");
}

// ---------------------------------------------------------------- the environment (G9)

fn secret_shaped(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.ends_with("_API_KEY") || upper.ends_with("_TOKEN") || upper.starts_with("OCTOS")
}

#[cfg(unix)]
#[test]
fn a_child_gets_only_the_allow_list_and_never_a_key_or_token() {
    // As if the shell's environment held provider keys and tokens (fake
    // values: no real key is ever read or printed here).
    let fake = "fake-value-for-the-test";
    let shell_env: Vec<(std::ffi::OsString, std::ffi::OsString)> = [
        ("PATH", "/usr/bin:/bin"),
        ("HOME", "/home/person"),
        ("LANG", "en_US.UTF-8"),
        ("TERM", "xterm-256color"),
        ("TMPDIR", "/tmp"),
        ("WAYLAND_DISPLAY", "wayland-0"),
        ("DISPLAY", ":0"),
        ("XAUTHORITY", "/home/person/.Xauthority"),
        ("MAKEPAD_WM_THEME_SPLASH", "dark"),
        ("OPENAI_API_KEY", fake),
        ("ANTHROPIC_API_KEY", fake),
        ("DEEPSEEK_API_KEY", fake),
        ("GITHUB_TOKEN", fake),
        ("HF_TOKEN", fake),
        ("CARGO_REGISTRY_TOKEN", fake),
        ("OCTOS_AUTH_TOKEN", fake),
        ("OCTOS_APP_CORE_DIR", "/core"),
        ("OCTOSENSE_SECRETS", "/vault"),
        ("OCTOSENSE_HOME", "/home/person/.octosense"),
        ("AWS_SECRET_ACCESS_KEY", fake),
        ("SOME_TOOLS_PRIVATE_SETTING", "x"),
    ]
    .iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    let mut cmd = Command::new("/usr/bin/env");
    cmd.env("STUDIO_HOST", "http://127.0.0.1:8765").env("MY_SERVICE_API_KEY", fake);
    scrub_env_from(&mut cmd, shell_env);
    let out = cmd.output().unwrap();
    let out = String::from_utf8_lossy(&out.stdout).to_string();
    let names: Vec<&str> = out.lines().map(|l| l.split('=').next().unwrap_or("")).collect();
    for name in &names {
        assert!(!secret_shaped(name) && !is_secret_var(name), "the child sees {name}");
    }
    assert!(!out.contains(fake), "no secret value reaches the child");
    for kept in ["PATH", "HOME", "LANG", "TERM", "TMPDIR", "WAYLAND_DISPLAY", "DISPLAY", "XAUTHORITY", "MAKEPAD_WM_THEME_SPLASH", "STUDIO_HOST"] {
        assert!(names.contains(&kept), "{kept} is passed on: {names:?}");
    }
    assert!(!names.contains(&"SOME_TOOLS_PRIVATE_SETTING"), "anything not on the list stays with the shell");
}

#[test]
fn the_allow_list_never_admits_a_secret_shaped_name() {
    for name in ["OPENAI_API_KEY", "anthropic_api_key", "GITHUB_TOKEN", "OCTOS_HOST_EXTERNAL_TOKEN", "OCTOSENSE_WORKSPACE", "OCTOSX", "MAKEPAD_WM_TOKEN", "XDG_SECRET", "CARGO_BUILD_API_KEY"] {
        assert!(is_secret_var(name) && !inherited_var(name), "{name}");
    }
    for name in ["PATH", "HOME", "LANG", "LC_ALL", "TERM", "TMPDIR", "DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "XAUTHORITY", "MAKEPAD_WM_ROOT", "CARGO_HOME", "RUSTUP_TOOLCHAIN"] {
        assert!(inherited_var(name), "{name}");
    }
}

#[test]
fn a_program_inside_the_octosense_home_stays_readable() {
    // The desktop builds process apps into `<OctoSense home>/build`: the
    // private deny must not take the app's own program away (the Terminal
    // crashed at start, unable to read its own bundle).
    let root = Path::new("/nonexistent/home");
    let mut p = home_rw_with_octosense_home(root);
    let build = root.join(".octosense/build/makepad");
    p.program = vec![build.clone()];
    let text = macos::profile(&p);
    let deny = text.find("(deny file-read* file-write* (subpath \"/nonexistent/home/.octosense\")").expect(&text);
    let program = text.rfind(&format!("(allow file-read* (subpath \"{}\"))", build.display())).expect(&text);
    assert!(deny < program, "its program is readable after the private deny\n{text}");
    assert!(!text[deny..].contains(&format!("(allow file-read* file-write* (subpath \"{}\"))", build.display())), "read-only, never writable\n{text}");
    // Nothing else inside the private dirs is reopened.
    let mut outside = p.clone();
    outside.program = vec![PathBuf::from("/nonexistent/programs")];
    assert!(!macos::profile(&outside).contains("even inside them"));
}

#[test]
fn a_program_path_that_is_or_holds_private_data_is_never_reopened() {
    // A dev setup whose checkout or target dir is the OctoSense home itself
    // (or holds it), or a program inside a sensitive directory, must not
    // reopen the home: peer tokens, other apps' jails and secrets, the kernel.
    let root = Path::new("/nonexistent/home");
    let octo = root.join(".octosense");
    let p = home_rw_with_octosense_home(root);
    for (program, why) in [
        (octo.clone(), "the OctoSense home itself"),
        (root.to_path_buf(), "a directory holding the OctoSense home"),
        (octo.join("apps/other/bin"), "another app's jail"),
        (octo.join("secrets"), "the secrets root"),
        (octo.join("app-peers"), "the peers' host tokens"),
        (root.join("octos-home/.octos/bin"), "the kernel's core dir"),
        (root.join("octos-home/tools"), "the kernel's home"),
    ] {
        assert!(!crate::sandbox::program_reopenable(&program, &p.private), "{why} must not be reopenable");
        let mut q = p.clone();
        q.program = vec![program.clone()];
        let text = macos::profile(&q);
        let deny = text.find("(deny file-read* file-write* (subpath \"/nonexistent/home/.octosense\")").expect(&text);
        assert!(!text[deny..].contains("even inside them"), "{why}: nothing reopened\n{text}");
        if program.starts_with(&octo) || program.starts_with(root.join("octos-home")) {
            assert!(text[deny..].contains("not reopened (it holds private data)"), "{why}: the skip is recorded\n{text}");
        }
    }
    assert!(crate::sandbox::program_reopenable(&octo.join("build/makepad"), &p.private));
}

#[cfg(target_os = "linux")]
#[test]
fn the_linux_rules_keep_a_program_inside_the_home_read_and_execute_only() {
    let scratch = Scratch::new("octoprog");
    let root = &scratch.0;
    let octo = root.join(".octosense");
    std::fs::create_dir_all(octo.join("build/makepad")).unwrap();
    std::fs::create_dir_all(octo.join("apps/probe")).unwrap();
    let mut p = home_rw_with_octosense_home(root);
    let build = octo.join("build/makepad");
    p.program = vec![build.clone()];
    // Inside the home it keeps read and execute only (a cargo launch builds
    // outside the sandbox, so nothing ever writes it from inside).
    let rules = linux::rules(&p, 3);
    let kept: Vec<_> = rules.iter().filter(|r| r.path == build).collect();
    assert!(!kept.is_empty(), "the program stays reachable");
    let rx = linux::read_exec();
    for r in kept {
        assert_eq!(r.access & !rx, 0, "read and execute only, never write: {:#x}", r.access);
    }
    // The home itself as the program: dropped, nothing reopened.
    p.program = vec![octo.clone()];
    assert!(linux::rules(&p, 3).iter().all(|r| r.path != octo), "the OctoSense home is never granted");
}

#[cfg(unix)]
#[test]
fn a_linked_checkout_into_the_octosense_home_is_not_reopened() {
    // A checkout or target dir reached through a link must be judged by the
    // path it resolves to: a link to the OctoSense home reopens nothing.
    let scratch = Scratch::new("octolink");
    let root = &scratch.0;
    let octo = root.join(".octosense");
    std::fs::create_dir_all(octo.join("apps/probe")).unwrap();
    std::fs::create_dir_all(octo.join("build/makepad")).unwrap();
    let link = root.join("checkout");
    std::os::unix::fs::symlink(&octo, &link).unwrap();
    let mut p = home_rw_with_octosense_home(root);
    p.program = vec![link.clone()];
    let text = macos::profile(&p);
    assert!(!text.contains("even inside them"), "a link to the home reopens nothing\n{text}");
    // A link to the build dir itself is fine: it resolves strictly inside.
    let build_link = root.join("build-link");
    std::os::unix::fs::symlink(octo.join("build/makepad"), &build_link).unwrap();
    p.program = vec![build_link];
    assert!(macos::profile(&p).contains("even inside them"));
    #[cfg(target_os = "linux")]
    {
        p.program = vec![link];
        let real = crate::sandbox::resolved(&octo);
        assert!(linux::rules(&p, 3).iter().all(|r| r.path != real), "Landlock never grants the home through a link");
    }
}

// ------------------------------------------------ what the next build reads (ADR 0004 §3, review 2026-09-30)

/// The Terminal's `home:rw` with a checkout, target dir and cargo home
/// under the same home: all of them read-only after every grant.
fn home_rw_with_a_build(root: &Path) -> Policy {
    let mut p = home_rw_with_octosense_home(root);
    p.program = vec![root.join("src/OctoSense"), root.join("src/OctoSense/target/release")];
    p.read_only = vec![root.join("src/OctoSense"), root.join("src/OctoSense/target"), root.join(".cargo"), root.join(".rustup"), root.join("src/.cargo")];
    p
}

#[test]
fn the_macos_profile_keeps_the_build_read_only_after_every_grant() {
    let root = Path::new("/nonexistent/home");
    let text = macos::profile(&home_rw_with_a_build(root));
    let grant = text.find("(subpath \"/nonexistent/home\"))").expect(&text);
    let ro = text.find(";; what the next build reads or runs stays read-only").expect(&text);
    assert!(grant < ro, "the read-only rule comes after home:rw (the last match wins)\n{text}");
    for dir in [".cargo", ".rustup", "src/.cargo", "src/OctoSense", "src/OctoSense/target"] {
        assert!(text[ro..].contains(&format!("(subpath \"/nonexistent/home/{dir}\")")), "{dir}\n{text}");
    }
    // Login items not writable, last of all.
    let agents = text.find("(deny file-write* (subpath \"/nonexistent/home/Library/LaunchAgents\"))").expect(&text);
    assert!(ro < agents, "{text}");
    // A jail inside a read-only path stays writable.
    let mut inside = home_rw_with_a_build(root);
    inside.read_only.push(root.join(".octosense"));
    let text = macos::profile(&inside);
    assert!(text.contains(";; its own jail and secrets even there\n(allow file-write* (subpath \"/nonexistent/home/.octosense/apps/probe\")"), "{text}");
}

/// Proved on this machine: from inside the broad sandbox, nothing the next
/// build reads or runs can be written (the review's `.cargo/config.toml`
/// build wrapper), and login items cannot be added; the person's other
/// files still can.
#[cfg(target_os = "macos")]
#[test]
fn inside_the_terminals_sandbox_the_build_toolchain_and_login_items_are_out_of_reach() {
    if !sandbox_works_here() {
        eprintln!("no process sandbox on this machine; skipped");
        return;
    }
    let scratch = Scratch::new("buildro");
    let root = &scratch.0;
    for dir in ["src/OctoSense/target/release", "src/OctoSense/.cargo", ".cargo/bin", ".rustup", "Library/LaunchAgents", "Documents", ".octosense/apps/probe", ".octosense/secrets/probe"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("src/OctoSense/Cargo.toml"), "[workspace]").unwrap();
    let policy = home_rw_with_a_build(root);
    let sh = |script: String| run("/bin/sh", &["-c", &script], &policy);
    for target in [
        "src/OctoSense/.cargo/config.toml",
        "src/.cargo/config.toml",
        ".cargo/config.toml",
        ".cargo/bin/cargo",
        ".rustup/settings.toml",
        "src/OctoSense/target/release/terminal",
        "src/OctoSense/Cargo.toml",
        "Library/LaunchAgents/evil.plist",
    ] {
        let (ok, out) = sh(format!("echo '[build]\nrustc-wrapper = \"/tmp/x\"' > {}", root.join(target).display()));
        assert!(!ok, "{target} must not be writable: {out}");
    }
    let (ok, out) = sh(format!("mkdir {}", root.join("src/.cargo").display()));
    assert!(!ok, "no .cargo directory can be made above the checkout either: {out}");
    let (ok, out) = sh(format!("cat {}/src/OctoSense/Cargo.toml", root.display()));
    assert!(ok && out.contains("workspace"), "the checkout stays readable: {out}");
    let (ok, out) = sh(format!("echo x > {}/Documents/note.txt", root.display()));
    assert!(ok, "the person's own files stay writable under home:rw: {out}");
}

/// `terminal.run` follows what the Terminal's newest launch reported
/// (ADR 0004 §10, §12): not before a launch, not after an unsandboxed one.
#[test]
fn the_newest_launch_says_whether_an_app_ran_sandboxed() {
    assert_eq!(launch_state("probe-launches"), None);
    assert!(!launch_sandboxed("probe-launches"), "no launch yet: nothing to vouch for");
    note_launch("probe-launches", Some(&Applied::Sandboxed("ok".into())));
    assert!(launch_sandboxed("probe-launches"));
    note_launch("probe-launches", Some(&Applied::Unavailable("sandbox-exec is missing".into())));
    assert_eq!(launch_state("probe-launches"), Some(false), "an unsandboxed launch withdraws it");
    note_launch("probe-launches", None);
    assert!(!launch_sandboxed("probe-launches"), "no policy is no sandbox");
}
