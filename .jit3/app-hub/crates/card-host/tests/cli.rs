//! The built `card-host` answers its command line without starting the app
//! (App Hub#9): `--help` prints the usage and exits 0; a bad option prints it
//! and exits 2. Neither may open a window or keep running.
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn card_host(args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_card-host"))
        .args(args)
        // Never on anyone's screen, and no control surface, should the app
        // start after all.
        .env("MAKEPAD_HIDE_WINDOWS", "1")
        .env_remove("MAKEPAD_REMOTE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("card-host runs");
    let deadline = Instant::now() + Duration::from_secs(20);
    while child.try_wait().expect("card-host status").is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("card-host {args:?} was still running after 20s: it started the app");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    child.wait_with_output().expect("card-host output")
}

#[test]
fn help_prints_usage_and_exits_zero() {
    for flag in ["--help", "-h"] {
        let out = card_host(&[flag]);
        assert_eq!(out.status.code(), Some(0), "{flag}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        for documented in ["--bundle", "--app-data", "--allow-unsigned", "--stamp", "--system", "--static"] {
            assert!(stdout.contains(documented), "{flag}: usage lacks {documented}:\n{stdout}");
        }
        assert!(out.stderr.is_empty(), "{flag}: {}", String::from_utf8_lossy(&out.stderr));
    }
}

#[test]
fn a_bad_command_line_prints_usage_and_exits_two() {
    for args in [&["--bogus"][..], &["--bundle"], &["stray"], &["--static", "no-equals"]] {
        let out = card_host(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.starts_with("card-host: "), "{args:?}: {stderr}");
        assert!(stderr.contains("Usage:"), "{args:?}: {stderr}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
}
