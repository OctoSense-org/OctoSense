//! The sandbox, proved on this machine: a child started through
//! [`command`] under a jail-only policy reads its jail and is refused a
//! file beside it, a network connection other than the hub, and a child
//! process; and no child inherits a host token.

#![cfg_attr(not(unix), allow(unused))]

use super::*;
use std::io::Read;

// The test binary is a runner too, so a cargo launch is tested for real.
crate::runner_entry!();
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
        calls_per_turn: None,
        calls_per_day: None,
    };
    // The scratch root stands in for the person's home: closed but for the
    // jail. The probe's tools are its program.
    Policy::for_app(&app, root.join("apps/probe"), root.join("secrets/probe"), root, vec!["/bin".into(), "/usr/bin".into()], hub_port)
}

fn run(program: &str, args: &[&str], policy: &Policy) -> (bool, String) {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (mut cmd, applied) = command(Path::new(program), &args, Some(policy), false);
    assert!(matches!(applied, Some(Applied::Sandboxed(_))), "{applied:?}");
    let out = cmd.output().expect("the probe starts");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
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

    // Network: the hub on loopback, nothing else.
    let nc = |port: u16| run("/usr/bin/nc", &["-z", "-w", "2", "127.0.0.1", &port.to_string()], &policy);
    let (ok, out) = nc(hub_port);
    assert!(ok, "the hub is reachable: {out}");
    let (ok, out) = nc(other_port);
    assert!(!ok, "any other connection is refused: {out}");

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
    let (mut cmd, applied) = command(Path::new("/bin/cat"), &args, Some(&policy), false);
    let Some(Applied::Sandboxed(how)) = applied else { panic!("{applied:?}") };
    assert!(how.contains("no landlock (kernel lacks it): paths are not restricted") && how.contains("seccomp"), "{how}");
    let out = cmd.output().expect("the app starts without Landlock");
    assert!(out.status.success() && String::from_utf8_lossy(&out.stdout).contains("mine"));
    // seccomp still refuses a child process.
    let (mut cmd, _) = command(Path::new("/bin/sh"), &["-c".into(), "/bin/echo first; /bin/echo spawned".into()], Some(&policy), false);
    let out = cmd.output().expect("sh starts");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains("spawned"), "no child process without Landlock either: {text}");
}

/// A launch through cargo sandboxes only the app, as on macOS: the build
/// (here a shell standing in for cargo) runs outside, and the app starts
/// through the binary's runner under the policy (found on a real kernel:
/// a build inside the sandbox had the checkout, the target dir, the shell's
/// own binary, `~/.cargo` and `/tmp` writable, and no port rules).
#[cfg(target_os = "linux")]
#[test]
fn a_cargo_launch_sandboxes_the_app_alone_through_the_runner() {
    if !sandbox_works_here() {
        eprintln!("no process sandbox on this machine; skipped");
        return;
    }
    let scratch = Scratch::new("runner");
    let root = &scratch.0;
    std::fs::write(root.join("apps/probe/mine.txt"), "mine").unwrap();
    std::fs::write(root.join("outside.txt"), "not yours").unwrap();
    let mut policy = jail_only(root, 1);
    policy.cargo_env_unset = vec!["OCTOSENSE_WORKSPACE_FOR_TEST".into()];
    let runner = linux::runner_var();
    let script = format!(
        "cat {out} && echo BUILD-SIDE-READ; echo \"unset=${{OCTOSENSE_WORKSPACE_FOR_TEST:-gone}}\"; exec ${runner} /bin/sh -c 'cat {mine}; cat {out} || echo APP-REFUSED; echo \"app-unset=${{OCTOSENSE_WORKSPACE_FOR_TEST:-gone}}\"'",
        out = root.join("outside.txt").display(),
        mine = root.join("apps/probe/mine.txt").display(),
    );
    let (mut cmd, applied) = command(Path::new("/bin/sh"), &["-c".into(), script], Some(&Policy { processes: true, ..policy }), true);
    let Some(Applied::Sandboxed(how)) = applied else { panic!("{applied:?}") };
    assert!(how.contains("as cargo's runner"), "{how}");
    let out = cmd.env("OCTOSENSE_WORKSPACE_FOR_TEST", "/x").output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("BUILD-SIDE-READ") && text.contains("unset=/x"), "the build side runs outside the sandbox: {text}");
    assert!(text.contains("mine") && text.contains("APP-REFUSED"), "the app runs inside it: {text}");
    assert!(text.contains("app-unset=gone"), "the checkout's [env] host variables are removed: {text}");
}

/// A runner plan survives its file: every path byte, the unset list.
#[cfg(target_os = "linux")]
#[test]
fn a_runner_plan_reads_back_what_was_written() {
    let plan = linux::Plan {
        abi: 6,
        rules: vec![(std::ffi::CString::new("/a b/\nc").unwrap(), 0x7), (std::ffi::CString::new("/usr").unwrap(), 0xc)],
        net: true,
        hub_port: 8765,
        processes: false,
        unset: vec!["OCTOSENSE_WORKSPACE".into()],
    };
    assert_eq!(linux::Plan::from_text(&plan.to_text()), Some(plan));
    assert_eq!(linux::Plan::from_text("abi 3\n"), None, "not a plan");
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
        ("MAKEPAD", "vulkan"),
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
    for kept in ["PATH", "HOME", "LANG", "TERM", "TMPDIR", "WAYLAND_DISPLAY", "DISPLAY", "XAUTHORITY", "MAKEPAD_WM_THEME_SPLASH", "MAKEPAD", "STUDIO_HOST"] {
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
