#![cfg(target_os = "macos")]
//! Run the actual packaged helper protocol, without a network or live user profile.
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// The installer lock's folder for one test: a debug helper uses it instead
/// of the folder under the person's home (`OCTOSENSE_UPDATE_LOCK_DIR`, not in
/// release builds, whose helper locks in the real folder as it would ship).
struct Locks(tempfile::TempDir);
impl Locks {
    fn new() -> Self {
        let folder = tempfile::tempdir().unwrap();
        fs::set_permissions(folder.path(), fs::Permissions::from_mode(0o700)).unwrap();
        Self(folder)
    }
    fn apply(&self, command: &mut Command) {
        command.env("OCTOSENSE_UPDATE_LOCK_DIR", self.0.path());
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    locks: Locks,
    root: PathBuf,
    job: PathBuf,
    target: PathBuf,
    nonce: String,
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().canonicalize().unwrap();
    let nonce = uuid::Uuid::new_v4().to_string();
    let root = parent.join(format!(".octosense-update-{nonce}"));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let target = parent.join("OctoSense.app");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("untouched"), b"original").unwrap();
    let job = root.join("job.json");
    let ticket = serde_json::json!({"protocol":1,"nonce":nonce,"parent_pid":std::process::id(),
        "target":target,"old_digest":"","new_digest":"","new_tag":"desktop-v0.1.0-rc.6",
        "environment":{},"startup_ack":true,"receipt":root.join("receipt.json")});
    fs::write(&job, serde_json::to_vec(&ticket).unwrap()).unwrap();
    fs::set_permissions(&job, fs::Permissions::from_mode(0o600)).unwrap();
    Fixture {
        _temp: temp,
        locks: Locks::new(),
        root,
        job,
        target,
        nonce,
    }
}
fn start(f: &Fixture) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_octosense-update-helper"));
    f.locks.apply(&mut command);
    command
        .arg(&f.job)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}
fn ready(child: &mut Child) {
    let stdout = child.stdout.take().unwrap();
    let (send, recv) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = BufReader::new(stdout).read_line(&mut text);
        let _ = send.send(text);
    });
    let line = recv.recv_timeout(Duration::from_secs(5));
    if line.as_deref() != Ok("ready\n") {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert_eq!(line.unwrap(), "ready\n");
}
fn wait(child: &mut Child) -> std::process::ExitStatus {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(code) = child.try_wait().unwrap() {
            return code;
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("helper did not exit");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
/// The names in a folder, sorted.
fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn closing_commit_pipe_cancels_and_removes_only_the_staging_folder() {
    for instruction in [None, Some("commit wrong-nonce\n")] {
        let f = fixture();
        let mut child = start(&f);
        ready(&mut child);
        let mut pipe = child.stdin.take().unwrap();
        if let Some(text) = instruction {
            pipe.write_all(text.as_bytes()).unwrap();
        }
        drop(pipe);
        assert!(wait(&mut child).success());
        assert!(!f.root.exists());
        assert_eq!(fs::read(f.target.join("untouched")).unwrap(), b"original");
        // Its lock lives in the lock folder, not here: nothing is left beside
        // the app.
        assert_eq!(names(f.target.parent().unwrap()), ["OctoSense.app"]);
    }
}

/// The reviewer's attack, end to end: a same-user process plants a staging
/// folder with an unsigned app and its own job, names a host that already
/// exited, and commits. The installed app here is unsigned too.
struct Planted {
    _temp: tempfile::TempDir,
    locks: Locks,
    parent: PathBuf,
    job: PathBuf,
    target: PathBuf,
    candidate: PathBuf,
    profile: PathBuf,
    receipt: PathBuf,
    nonce: String,
}
/// Records whether it was opened with the rollback signal.
const OPENED: &str =
    "#!/bin/sh\nprintf '%s' \"${OCTOSENSE_UPDATE_ROLLED_BACK:+rolled-back}\" > \"$OCTOSENSE_HOME/opened\"\n";
const PLANTED: &str = "#!/bin/sh\n: > \"$OCTOSENSE_HOME/planted-app-ran\"\n";
fn app(path: &Path, version: &str, program: &str) {
    fs::create_dir_all(path.join("Contents/MacOS")).unwrap();
    fs::write(path.join("Contents/Info.plist"), format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>org.octosense.desktop</string>
<key>CFBundleExecutable</key><string>octosense</string>
<key>CFBundleShortVersionString</key><string>{version}</string></dict></plist>"#)).unwrap();
    let exe = path.join("Contents/MacOS/octosense");
    fs::write(&exe, program).unwrap();
    fs::set_permissions(exe, fs::Permissions::from_mode(0o755)).unwrap();
}
fn planted() -> Planted {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().canonicalize().unwrap();
    let nonce = uuid::Uuid::new_v4().to_string();
    let root = parent.join(format!(".octosense-update-{nonce}"));
    let updates = parent.join("updates");
    let profile = parent.join("profile");
    for folder in [&root, &updates] {
        fs::create_dir(folder).unwrap();
        fs::set_permissions(folder, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::create_dir(&profile).unwrap();
    let target = parent.join("OctoSense.app");
    let candidate = root.join("candidate.app");
    app(&target, "0.1.0-rc.5", OPENED);
    app(&candidate, "0.1.0-rc.6", PLANTED);
    // A host that already exited, as after a real commit.
    let mut gone = Command::new("/usr/bin/true").spawn().unwrap();
    let parent_pid = gone.id();
    gone.wait().unwrap();
    let receipt = updates.join("last-install.json");
    let job = root.join("job.json");
    let ticket = serde_json::json!({"protocol":1,"nonce":nonce,"parent_pid":parent_pid,
        "target":target,"old_digest":"planted","new_digest":"planted","new_tag":"desktop-v0.1.0-rc.6",
        "environment":{"OCTOSENSE_HOME":profile},"startup_ack":true,"receipt":receipt});
    fs::write(&job, serde_json::to_vec(&ticket).unwrap()).unwrap();
    fs::set_permissions(&job, fs::Permissions::from_mode(0o600)).unwrap();
    Planted {
        _temp: temp,
        locks: Locks::new(),
        parent,
        job,
        target,
        candidate,
        profile,
        receipt,
        nonce,
    }
}
/// Runs the real helper on a planted job through its commit; returns its stderr.
fn commit_planted(p: &Planted, fixtures_seam: bool) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_octosense-update-helper"));
    p.locks.apply(&mut command);
    command
        .arg(&p.job)
        .env_remove("OCTOSENSE_UPDATE_UNSIGNED_FIXTURES")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if fixtures_seam {
        command.env("OCTOSENSE_UPDATE_UNSIGNED_FIXTURES", "1");
    }
    let mut child = command.spawn().unwrap();
    ready(&mut child);
    writeln!(child.stdin.as_mut().unwrap(), "commit {}", p.nonce).unwrap();
    child.stdin.take();
    let until = Instant::now() + Duration::from_secs(60);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= until {
            let _ = child.kill();
            panic!("helper did not finish");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    String::from_utf8_lossy(&output.stderr).into_owned()
}
/// Generous, for a loaded machine: it returns as soon as the file appears.
fn wait_for(path: &Path) -> Option<String> {
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until {
        if let Ok(text) = fs::read_to_string(path) {
            return Some(text);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}
fn assert_untouched(p: &Planted) {
    assert_eq!(
        fs::read_to_string(p.target.join("Contents/MacOS/octosense")).unwrap(),
        OPENED
    );
    assert_eq!(
        fs::read_to_string(p.candidate.join("Contents/MacOS/octosense")).unwrap(),
        PLANTED
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(!p.profile.join("planted-app-ran").exists());
}

#[test]
fn a_planted_unsigned_app_is_never_installed_or_launched() {
    let p = planted();
    let error = commit_planted(&p, false);
    // The signature gate stops it: an unsigned installation vouches for nothing.
    assert!(error.contains("unsigned or ad-hoc signed"), "{error}");
    assert_untouched(&p);
    // Nothing verified to reopen, so nothing ran; the receipt says why.
    assert!(!p.profile.join("opened").exists());
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&p.receipt).unwrap()).unwrap();
    assert_eq!(receipt["state"], "failed");
    // And no lock file or anything else was left beside the app.
    let staging = format!(".octosense-update-{}", p.nonce);
    assert_eq!(
        names(&p.parent),
        [staging.as_str(), "OctoSense.app", "profile", "updates"]
    );
}

/// Run in both profiles: `cargo test` (debug) and `cargo test --release`.
/// Only a debug helper honours the unsigned-fixture setting; a release
/// helper, given the same setting, still refuses at the signature gate.
#[test]
fn the_unsigned_fixture_setting_is_honoured_by_debug_builds_only() {
    let p = planted();
    let error = commit_planted(&p, true);
    if cfg!(debug_assertions) {
        // Past the signature gate, the planted fingerprints stop it, and the
        // unchanged app reopens with the rollback signal.
        assert!(error.contains("changed after it was prepared"), "{error}");
        assert_eq!(
            wait_for(&p.profile.join("opened")).as_deref(),
            Some("rolled-back")
        );
    } else {
        assert!(error.contains("unsigned or ad-hoc signed"), "{error}");
        std::thread::sleep(Duration::from_millis(300));
        assert!(!p.profile.join("opened").exists());
    }
    assert_untouched(&p);
}
#[test]
fn commit_waits_for_parent_exit_and_a_second_helper_cannot_race_it() {
    let f = fixture();
    let mut child = start(&f);
    ready(&mut child);
    let mut other = start(&f);
    let denied = wait(&mut other);
    let output = other.wait_with_output().unwrap();
    assert!(!denied.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Another update"));
    writeln!(child.stdin.as_mut().unwrap(), "commit {}", f.nonce).unwrap();
    child.stdin.take();
    let until = Instant::now() + Duration::from_secs(5);
    while !f.root.join("state.json").exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    let state = fs::read_to_string(f.root.join("state.json"));
    let unchanged = fs::read(f.target.join("untouched"));
    let alive = child.try_wait().unwrap().is_none();
    let _ = child.kill(); // this test owns the helper; model a helper interruption
    let _ = child.wait();
    assert!(alive);
    assert!(state.unwrap().contains("waiting_for_exit"));
    assert_eq!(unchanged.unwrap(), b"original");
    assert!(f.root.exists(), "interruption retains its recovery journal");
}
