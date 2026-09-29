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
