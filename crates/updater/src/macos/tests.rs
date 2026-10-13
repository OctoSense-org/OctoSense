use super::*;
use tempfile::TempDir;

/// A short stop-wait for tests; production waits `STOP_WAIT`.
const QUICK: Duration = Duration::from_millis(300);
/// The installed app: records whether it was reopened with the rollback signal.
const OPENED: &str =
    "#!/bin/sh\nprintf '%s' \"${OCTOSENSE_UPDATE_ROLLED_BACK:+rolled-back}\" > \"$OCTOSENSE_HOME/opened\"\n";
/// A new app that must never start: records it, then fails.
const NEVER: &str = "#!/bin/sh\n: > \"$OCTOSENSE_HOME/new-started\"\nexit 7\n";

struct Fixture {
    _temp: TempDir,
    ticket: Ticket,
    job: PathBuf,
    candidate: PathBuf,
}
impl Fixture {
    fn profile(&self) -> PathBuf {
        PathBuf::from(&self.ticket.environment["OCTOSENSE_HOME"])
    }
    fn root(&self) -> &Path {
        self.job.parent().unwrap()
    }
    /// Recompute the prepared digests after changing a fixture bundle.
    fn prepare_again(&mut self) {
        self.ticket.old_digest = digest_tree(&self.ticket.target).unwrap();
        self.ticket.new_digest = digest_tree(&self.candidate).unwrap();
        write_json(&self.job, &self.ticket).unwrap();
    }
    fn receipt(&self) -> serde_json::Value {
        serde_json::from_slice(&fs::read(&self.ticket.receipt).unwrap()).unwrap()
    }
    fn journal(&self) -> String {
        fs::read_to_string(self.root().join("state.json")).unwrap()
    }
}

fn bundle(path: &Path, version: &str, program: &str) {
    fs::create_dir_all(path.join("Contents/MacOS")).unwrap();
    fs::write(path.join("Contents/Info.plist"), format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{BUNDLE_ID}</string>
<key>CFBundleExecutable</key><string>octosense</string>
<key>CFBundleShortVersionString</key><string>{version}</string></dict></plist>"#)).unwrap();
    let exe = path.join("Contents/MacOS/octosense");
    fs::write(&exe, program).unwrap();
    fs::set_permissions(exe, fs::Permissions::from_mode(0o755)).unwrap();
}

/// Ad-hoc signing (`codesign -s -`, part of macOS): a valid signature with
/// no developer identity, whose designated requirement is its own hash.
fn ad_hoc_sign(bundle: &Path, identifier: &str) {
    let signed = Command::new("/usr/bin/codesign")
        .args(["--force", "--sign", "-", "--identifier", identifier])
        .arg(bundle)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(signed.success(), "codesign could not sign the fixture");
}

fn fixtures(_: &Path) -> Result<Trust> {
    Ok(Trust::Fixtures)
}

/// The body of `sidecar`'s process; a no-op in the normal test run.
#[test]
fn sidecar_process() {
    if let Some(millis) = std::env::var("OCTOSENSE_TEST_SIDECAR_MS")
        .ok()
        .and_then(|value| value.parse().ok())
    {
        std::thread::sleep(Duration::from_millis(millis));
    }
}

/// A native process running from inside `bundle` for `millis`, like the
/// kernel sidecar: this test executable, copied in. (macOS kills copies of
/// its own platform binaries, such as /bin/sleep, under launch constraints.)
fn sidecar(bundle: &Path, millis: u64) -> Child {
    let exe = bundle.join("Contents/MacOS/octos-kernel");
    if !exe.exists() {
        fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
    }
    let child = Command::new(&exe)
        .args([
            "--exact",
            "macos::tests::sidecar_process",
            "--test-threads=1",
        ])
        .env("OCTOSENSE_TEST_SIDECAR_MS", millis.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(150));
    child
}

/// A marker a launched fixture writes. Generous, for a loaded machine: it
/// returns as soon as the file appears.
fn wait_for(path: &Path) -> String {
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until {
        if let Ok(text) = fs::read_to_string(path) {
            return text;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("{} was not written", path.display());
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().canonicalize().unwrap();
    let nonce = uuid::Uuid::new_v4().to_string();
    let root = parent.join(format!(".octosense-update-{nonce}"));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let target = parent.join("OctoSense Renamed.app");
    let candidate = root.join("candidate.app");
    bundle(&target, "0.1.0-rc.5", "#!/bin/sh\nexit 0\n");
    bundle(&candidate, "0.1.0-rc.6", "#!/bin/sh\nexit 7\n");
    let receipt_dir = parent.join("updates");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&receipt_dir)
        .unwrap();
    let ticket = Ticket {
        protocol: 1,
        nonce,
        parent_pid: std::process::id(),
        old_digest: digest_tree(&target).unwrap(),
        new_digest: digest_tree(&candidate).unwrap(),
        new_tag: "desktop-v0.1.0-rc.6".into(),
        target,
        environment: BTreeMap::from([(
            "OCTOSENSE_HOME".into(),
            parent.join("profile").display().to_string(),
        )]),
        startup_ack: true,
        receipt: receipt_dir.join("last-install.json"),
    };
    fs::create_dir(ticket.environment.get("OCTOSENSE_HOME").unwrap()).unwrap();
    fs::write(
        Path::new(&ticket.environment["OCTOSENSE_HOME"]).join("note.md"),
        "Keep this note 升级",
    )
    .unwrap();
    let job = root.join("job.json");
    write_json(&job, &ticket).unwrap();
    Fixture {
        _temp: temp,
        ticket,
        job,
        candidate,
    }
}

#[test]
fn renamed_installations_are_detected_but_loose_binaries_are_not() {
    let f = fixture();
    assert_eq!(
        bundle_for_executable(&f.ticket.target.join("Contents/MacOS/octosense")).unwrap(),
        f.ticket.target
    );
    for relative in ["octosense", "Contents/octosense", "Contents/MacOS/other"] {
        assert!(bundle_for_executable(&f.ticket.target.join(relative)).is_err());
    }
    validate_bundle(&f.candidate, Some("0.1.0-rc.6")).unwrap();
    assert!(validate_bundle(&f.candidate, Some("0.1.0-rc.7")).is_err());
}

#[test]
fn tampering_and_outside_symlinks_are_refused_before_exchange() {
    let f = fixture();
    fs::write(
        f.candidate.join("Contents/extra"),
        b"changed after verification",
    )
    .unwrap();
    assert!(
        verify_before_exchange(&f.ticket, &f.candidate, &Trust::Fixtures, QUICK)
            .unwrap_err()
            .contains("changed")
    );
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
    std::os::unix::fs::symlink(
        f.job.parent().unwrap().join("job.json"),
        f.candidate.join("Contents/escape"),
    )
    .unwrap();
    assert!(digest_tree(&f.candidate).unwrap_err().contains("outside"));
}

#[test]
fn failed_launch_rolls_back_without_touching_app_data() {
    let mut f = fixture();
    bundle(&f.ticket.target, "0.1.0-rc.5", OPENED);
    f.prepare_again();
    install_committed(&f.ticket, &f.job, &f.candidate, &fixtures, QUICK).unwrap();
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
    assert_eq!(digest_tree(&f.candidate).unwrap(), f.ticket.new_digest);
    assert_eq!(f.receipt()["state"], "rolled_back");
    assert!(f.receipt()["message"]
        .as_str()
        .unwrap()
        .contains("exited before startup"));
    assert_eq!(wait_for(&f.profile().join("opened")), "rolled-back");
    assert_eq!(
        fs::read_to_string(f.profile().join("note.md")).unwrap(),
        "Keep this note 升级"
    );
}

#[test]
fn interrupted_exchange_can_be_recovered_on_either_side_and_repeated() {
    let f = fixture();
    let restore = || {
        restore_previous(
            &f.ticket,
            &f.job,
            &f.candidate,
            &Trust::Fixtures,
            "Restored.",
        )
    };
    restore().unwrap(); // interruption before swap
    exchange(&f.ticket.target, &f.candidate).unwrap(); // interruption after swap
    assert!(f.ticket.target.is_dir() && f.candidate.is_dir());
    restore().unwrap();
    restore().unwrap();
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
    fs::write(f.candidate.join("Contents/extra"), b"unexpected").unwrap();
    assert!(restore().is_err());
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
}

#[test]
fn tickets_require_private_paths_and_only_known_relaunch_settings() {
    let mut f = fixture();
    assert_eq!(read_ticket(&f.job).unwrap().target, f.ticket.target);
    f.ticket
        .environment
        .insert("DYLD_INSERT_LIBRARIES".into(), "foreign".into());
    write_json(&f.job, &f.ticket).unwrap();
    assert!(read_ticket(&f.job).unwrap_err().contains("setting"));
    f.ticket.environment.remove("DYLD_INSERT_LIBRARIES");
    f.ticket.target = f
        .ticket
        .target
        .parent()
        .unwrap()
        .join("outside/OctoSense.app");
    write_json(&f.job, &f.ticket).unwrap();
    assert!(read_ticket(&f.job).is_err());
    fs::set_permissions(&f.job, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(read_ticket(&f.job).unwrap_err().contains("file"));
}

#[test]
fn relaunch_uses_the_explicit_profile_and_strips_inherited_configuration() {
    let mut f = fixture();
    bundle(
        &f.ticket.target,
        "0.1.0-rc.5",
        "#!/bin/sh\nprintf '%s' \"$OCTOSENSE_HOME\" > \"$OCTOSENSE_HOME/observed\"\n",
    );
    let mut child = launch(&f.ticket, None).unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(
        fs::read_to_string(Path::new(&f.ticket.environment["OCTOSENSE_HOME"]).join("observed"))
            .unwrap(),
        f.ticket.environment["OCTOSENSE_HOME"]
    );
    f.ticket.environment.clear();
    assert!(!environment_keys().contains(&"DYLD_INSERT_LIBRARIES"));
    assert!(!environment_keys().contains(&"OCTOSENSE_RELEASE_TAG"));
}

#[test]
fn an_in_use_native_bundle_is_not_replaced() {
    let f = fixture();
    let mut child = sidecar(&f.ticket.target, 10_000);
    let result = ensure_bundle_stopped(&f.ticket.target);
    let _ = child.kill();
    let _ = child.wait();
    assert!(result.unwrap_err().contains("Another process"));
    ensure_bundle_stopped(&f.ticket.target).unwrap();
}

// F1: only code with the installed app's own signing identity is installed
// or launched, and a source build cannot vouch for anything.
#[test]
fn installations_without_a_developer_identity_cannot_vouch_for_an_update() {
    let f = fixture();
    let refusal = |bundle: &Path| codesign::Identity::installed(bundle).err().unwrap();
    // Unsigned, like a source package; then ad-hoc signed, like every local build.
    assert!(refusal(&f.ticket.target).contains("source or development build"));
    ad_hoc_sign(&f.ticket.target, BUNDLE_ID);
    assert!(refusal(&f.ticket.target).contains("source or development build"));
    // A signature that no longer matches the bundle is not an identity either.
    fs::write(f.ticket.target.join("Contents/Info.plist"), b"modified").unwrap();
    assert!(!refusal(&f.ticket.target).contains("source or development build"));
}

#[test]
fn unsigned_and_foreign_updates_are_refused_before_the_exchange_and_before_launch() {
    for foreign in [false, true] {
        let mut f = fixture();
        bundle(&f.ticket.target, "0.1.0-rc.5", OPENED);
        ad_hoc_sign(&f.ticket.target, BUNDLE_ID);
        bundle(&f.candidate, "0.1.0-rc.6", NEVER);
        if foreign {
            // Validly signed, but by another identity: its own code hash and identifier.
            ad_hoc_sign(&f.candidate, "org.example.other");
        }
        f.prepare_again();
        let installed = f.ticket.target.clone();
        let identity = |_: &Path| codesign::Identity::ad_hoc_fixture(&installed).map(Trust::Signed);
        let expected = if foreign {
            "installed app's developer"
        } else {
            "is not signed"
        };
        // The identity admits the installed app itself, so the refusals below
        // are about the update, not a broken check.
        identity(&installed).unwrap().admit(&installed).unwrap();
        // Before the exchange: refused, nothing moved, and the person gets
        // the installed app back with the rollback signal (F2).
        let error =
            install_committed(&f.ticket, &f.job, &f.candidate, &identity, QUICK).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
        assert_eq!(digest_tree(&f.candidate).unwrap(), f.ticket.new_digest);
        assert_eq!(wait_for(&f.profile().join("opened")), "rolled-back");
        assert_eq!(f.receipt()["state"], "failed");
        // Before launch: even at the installed path, the update is not started.
        let trust = identity(&installed).unwrap();
        exchange(&f.ticket.target, &f.candidate).unwrap();
        let refused = launch_verified(&f.ticket, Some(&f.job), &trust)
            .err()
            .unwrap();
        assert!(refused.contains(expected), "{refused}");
        std::thread::sleep(Duration::from_millis(200));
        assert!(!f.profile().join("new-started").exists());
    }
}

#[test]
fn an_app_changed_after_its_checks_is_moved_back_out_not_left_installed() {
    let mut f = fixture();
    bundle(&f.ticket.target, "0.1.0-rc.5", OPENED);
    ad_hoc_sign(&f.ticket.target, BUNDLE_ID);
    bundle(&f.candidate, "0.1.0-rc.6", NEVER);
    f.prepare_again();
    let trust = codesign::Identity::ad_hoc_fixture(&f.ticket.target)
        .map(Trust::Signed)
        .unwrap();
    // As if another process replaced the staged app's contents after its
    // checks and before the exchange: what is now installed is unverified.
    exchange(&f.ticket.target, &f.candidate).unwrap();
    fs::write(f.ticket.target.join("Contents/planted"), b"swapped in late").unwrap();
    finish_exchanged(&f.ticket, &f.job, &f.candidate, &trust, QUICK).unwrap();
    // It never ran, the previous app is installed again and reopened, and
    // the unverified bundle sits in the staging folder for the sweep.
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
    assert!(f.candidate.join("Contents/planted").is_file());
    assert_eq!(wait_for(&f.profile().join("opened")), "rolled-back");
    assert!(!f.profile().join("new-started").exists());
    assert_eq!(f.receipt()["state"], "rolled_back");
    assert!(f.journal().contains("rolled_back"));
}

/// Real Developer ID signatures cannot be fixtured without a certificate.
/// Point OCTOSENSE_TEST_SIGNED_APP at a copy of a Developer-ID-signed app (as
/// OctoSense releases are signed) and OCTOSENSE_TEST_OTHER_TEAM_APP at a
/// signed app from another team, then run
/// `cargo test -p octosense-updater --lib -- --ignored real_developer_id`.
#[test]
#[ignore = "needs copies of two Developer-ID-signed apps"]
fn real_developer_id_identities_admit_their_own_team_only() {
    let app = |name: &str| PathBuf::from(std::env::var_os(name).expect(name));
    let (signed, other) = (
        app("OCTOSENSE_TEST_SIGNED_APP"),
        app("OCTOSENSE_TEST_OTHER_TEAM_APP"),
    );
    let identity = match codesign::Identity::installed(&signed) {
        Ok(identity) => identity,
        Err(error) => panic!("a Developer ID signed app was not accepted: {error}"),
    };
    assert_eq!(identity.admit(&signed).unwrap(), identity.version());
    assert!(identity
        .admit(&other)
        .unwrap_err()
        .contains("installed app's developer"));
    let temp = tempfile::tempdir().unwrap();
    let tampered = temp.path().join("tampered.app");
    let copied = Command::new("/usr/bin/ditto")
        .arg(&signed)
        .arg(&tampered)
        .status()
        .unwrap();
    assert!(copied.success());
    identity.admit(&tampered).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(tampered.join("Contents/Info.plist"))
        .unwrap()
        .write_all(b"\n")
        .unwrap();
    assert!(identity
        .admit(&tampered)
        .unwrap_err()
        .contains("invalid code signature"));
}

// F2: a failure after the commit never leaves the person without an app.
#[test]
fn bundled_children_are_waited_out_and_a_failure_before_the_exchange_reopens_the_app() {
    let mut f = fixture();
    bundle(&f.ticket.target, "0.1.0-rc.5", OPENED);
    // A sidecar that is still shutting down is waited out...
    let mut stopping = sidecar(&f.ticket.target, 800);
    let reaped = std::thread::spawn(move || stopping.wait());
    f.prepare_again();
    assert!(ensure_bundle_stopped(&f.ticket.target).is_err());
    wait_until_stopped(&f.ticket.target, Duration::from_secs(10)).unwrap();
    reaped.join().unwrap().unwrap();
    // ...one that stays makes the installation fail, and the unchanged app
    // is reopened with the rollback signal, its receipt saying why.
    let mut stuck = sidecar(&f.ticket.target, 30_000);
    let error = install_committed(&f.ticket, &f.job, &f.candidate, &fixtures, QUICK).unwrap_err();
    let _ = stuck.kill();
    let _ = stuck.wait();
    assert!(error.contains("Another process"), "{error}");
    assert_eq!(wait_for(&f.profile().join("opened")), "rolled-back");
    assert_eq!(f.receipt()["state"], "failed");
    assert!(f.journal().contains("not_installed"));
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
    // A staged update that changed after preparation: the same outcome.
    fs::remove_file(f.profile().join("opened")).unwrap();
    fs::write(f.candidate.join("Contents/extra"), b"changed").unwrap();
    let error = install_committed(&f.ticket, &f.job, &f.candidate, &fixtures, QUICK).unwrap_err();
    assert!(error.contains("changed after it was prepared"), "{error}");
    assert_eq!(wait_for(&f.profile().join("opened")), "rolled-back");
}

#[test]
fn an_installation_without_a_trusted_identity_reopens_nothing() {
    let mut f = fixture();
    bundle(&f.ticket.target, "0.1.0-rc.5", OPENED);
    f.prepare_again();
    let untrusted = |_: &Path| -> Result<Trust> { Err("Not signed.".into()) };
    let error = install_committed(&f.ticket, &f.job, &f.candidate, &untrusted, QUICK).unwrap_err();
    assert!(error.contains("Not signed."));
    std::thread::sleep(Duration::from_millis(200));
    assert!(!f.profile().join("opened").exists());
    assert_eq!(f.receipt()["state"], "failed");
}

#[test]
fn the_next_start_reports_an_unseen_failure_once() {
    let f = fixture();
    let cache = f.ticket.receipt.parent().unwrap();
    assert_eq!(take_install_notice(cache), None);
    status(&f.ticket, "complete", "Installed.").unwrap();
    assert_eq!(take_install_notice(cache), None);
    status(&f.ticket, "failed", "The update was not installed.\nWhy.").unwrap();
    let (title, message) = take_install_notice(cache).unwrap();
    assert_eq!(title, "Update not installed");
    assert_eq!(message, "The update was not installed.Why.");
    assert_eq!(take_install_notice(cache), None);
    status(&f.ticket, "cancelled", "").unwrap();
    assert_eq!(take_install_notice(cache).unwrap().0, "Update cancelled");
}

fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// The lock folder a test uses: `LOCK_DIR` under a stand-in home inside the
/// fixture's temporary folder. Unit tests never resolve the real home.
fn lock_folder(f: &Fixture) -> PathBuf {
    let home = f.ticket.target.parent().unwrap().join("home");
    private_folder(&home.join(LOCK_DIR)).unwrap()
}

fn lock_name(target: &Path) -> String {
    let hash = format!("{:x}", Sha256::digest(target.as_os_str().as_bytes()));
    format!("octosense-install-{}.lock", &hash[..16])
}

// F3 and the review's lock items: the lock is a file under the person's home
// (which the process-app sandbox closes), named after the app's path, and
// shared by every profile of this user. Nothing is created beside the app,
// and nobody else can hold it.
#[test]
fn the_install_lock_admits_one_installer_creates_nothing_and_ignores_other_locks() {
    let f = fixture();
    let folder = f.ticket.target.parent().unwrap();
    let locks = lock_folder(&f);
    let before = names(folder);
    // Anyone who can read the installation folder can lock the folder
    // itself, and a sandboxed app can write the shared temporary folder,
    // where the previous lock lived. Neither blocks an update.
    let shared = File::open(folder).unwrap();
    assert_eq!(
        unsafe { libc::flock(shared.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let elsewhere = tempfile::tempdir().unwrap();
    let decoy = File::create(elsewhere.path().join(lock_name(&f.ticket.target))).unwrap();
    assert_eq!(
        unsafe { libc::flock(decoy.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let lock = InstallLock::acquire_in(&locks, &f.ticket.target).unwrap();
    // The installer of any other profile of this user waits its turn: the
    // lock does not depend on the profile.
    assert!(InstallLock::acquire_in(&locks, &f.ticket.target)
        .err()
        .unwrap()
        .contains("Another update"));
    // Another installation has its own lock.
    let _other = InstallLock::acquire_in(&locks, &folder.join("Other.app")).unwrap();
    assert_eq!(names(folder), before);
    assert!(locks.join(lock_name(&f.ticket.target)).is_file());
    drop(lock);
    // Released on drop. A child another test thread is forking shares the
    // descriptor until it execs (close-on-exec), so allow a moment.
    let until = Instant::now() + Duration::from_secs(2);
    let again = loop {
        match InstallLock::acquire_in(&locks, &f.ticket.target) {
            Err(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(10)),
            again => break again,
        }
    };
    assert!(again.is_ok());
    assert_eq!(names(folder), before);
}

#[test]
fn the_lock_folder_is_created_owner_only_and_must_be_private() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap();
    // Created with every missing parent, owner-only.
    let created = private_folder(&home.join(LOCK_DIR)).unwrap();
    for folder in [created.as_path(), created.parent().unwrap()] {
        assert_eq!(fs::metadata(folder).unwrap().mode() & 0o777, 0o700);
    }
    // A folder others can read is refused, by the folder check and the lock.
    let open = home.join("open");
    fs::create_dir(&open).unwrap();
    fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(private_folder(&open).is_err());
    assert!(InstallLock::acquire_in(&open, &home.join("OctoSense.app")).is_err());
    // So is a link to a private folder, with or without a trailing slash.
    let link = home.join("link");
    std::os::unix::fs::symlink(&created, &link).unwrap();
    assert!(private_dir(&link).is_err());
    assert!(private_dir(Path::new(&format!("{}/", link.display()))).is_err());
    assert!(private_dir(Path::new(&format!("{}/", created.display()))).is_ok());
}

#[test]
fn the_home_folder_comes_from_the_user_database() {
    let home = user_home().unwrap();
    assert!(home.is_absolute() && home.is_dir(), "{}", home.display());
}

/// A retained update beside `target`, its job written at `written`.
fn retained(
    f: &Fixture,
    target: &Path,
    phase: Option<&str>,
    written: SystemTime,
    new_digest: &str,
) -> PathBuf {
    let nonce = uuid::Uuid::new_v4().to_string();
    let root = target
        .parent()
        .unwrap()
        .join(format!(".octosense-update-{nonce}"));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    bundle(&root.join("candidate.app"), "0.1.0-rc.4", "#!/bin/sh\n");
    let mut ticket = serde_json::to_value(&f.ticket).unwrap();
    ticket["nonce"] = nonce.into();
    ticket["target"] = target.to_str().unwrap().into();
    ticket["new_digest"] = new_digest.into();
    write_json(&root.join("job.json"), &ticket).unwrap();
    if let Some(phase) = phase {
        super::phase(&root, phase).unwrap();
    }
    fs::File::options()
        .write(true)
        .open(root.join("job.json"))
        .unwrap()
        .set_modified(written)
        .unwrap();
    root
}

// F4: retained updates are swept down to the one backup `--restore` needs.
#[test]
fn the_sweep_keeps_exactly_one_backup() {
    let f = fixture();
    let folder = f.ticket.target.parent().unwrap().to_path_buf();
    let now = SystemTime::now();
    let day = Duration::from_secs(24 * 60 * 60);
    // None of these installed the app that is installed now, so the sweep
    // falls back to their jobs' modification times.
    let update = f.ticket.new_digest.clone();
    let retained = |target: &Path, phase: Option<&str>, age: Duration| {
        retained(&f, target, phase, now - age, &update)
    };
    let target = f.ticket.target.clone();
    let oldest = retained(&target, Some("complete"), 3 * day);
    let interrupted = retained(&target, Some("starting"), 5 * day / 2);
    let newest = retained(&target, Some("complete"), 2 * day);
    let rolled_back = retained(&target, Some("rolled_back"), day);
    let failed_just_now = retained(&target, Some("not_installed"), Duration::from_secs(60));
    let other_installation = retained(&folder.join("Other.app"), Some("complete"), 4 * day);
    let preparing = folder.join(format!(".octosense-update-{}", uuid::Uuid::new_v4()));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&preparing)
        .unwrap();
    // A link out of a retained update is removed, never followed.
    std::os::unix::fs::symlink(f.profile().join("note.md"), oldest.join("note-link")).unwrap();
    let lock = InstallLock::acquire_in(&lock_folder(&f), &target).unwrap();
    assert_eq!(sweep_retained(&target, &lock, now), 3);
    for gone in [&oldest, &interrupted, &rolled_back] {
        assert!(!gone.exists(), "{} was kept", gone.display());
    }
    let backups = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|root| {
            read_ticket(&root.join("job.json")).is_ok_and(|ticket| ticket.target == target)
                && may_hold_previous_app(root)
        })
        .collect::<Vec<_>>();
    assert_eq!(backups, std::slice::from_ref(&newest));
    // Recent unused attempts, another installation's backups, a folder still
    // being prepared and everything outside stay.
    for kept in [
        &newest,
        &failed_just_now,
        &other_installation,
        &preparing,
        &f.root().to_path_buf(),
    ] {
        assert!(kept.is_dir(), "{} was removed", kept.display());
    }
    assert_eq!(
        fs::read_to_string(f.profile().join("note.md")).unwrap(),
        "Keep this note 升级"
    );
    assert_eq!(sweep_retained(&target, &lock, now), 0);
    // Ten minutes on, the failed attempt and the never-committed one go too.
    assert_eq!(
        sweep_retained(&target, &lock, now + 2 * DISPOSABLE_AFTER),
        2
    );
    assert!(newest.is_dir() && !failed_just_now.exists() && !f.root().exists());
}

#[test]
fn the_sweep_keeps_the_installed_apps_backup_after_the_clock_went_back() {
    let f = fixture();
    let target = f.ticket.target.clone();
    let now = SystemTime::now();
    let day = Duration::from_secs(24 * 60 * 60);
    let installed = digest_tree(&target).unwrap();
    // The backup of the update installed now was written first; an older
    // installation's backup carries a later time, written while the clock
    // ran ahead. Modification times alone would keep the wrong one.
    let needed = retained(&f, &target, Some("complete"), now - 2 * day, &installed);
    let stale = retained(&f, &target, Some("complete"), now - day, "an older update");
    let lock = InstallLock::acquire_in(&lock_folder(&f), &target).unwrap();
    assert_eq!(sweep_retained(&target, &lock, now), 1);
    assert!(needed.is_dir(), "the backup --restore needs was removed");
    assert!(!stale.exists());
}

// F5: every permission bit is fingerprinted; setuid and setgid are refused.
#[test]
fn permission_bits_are_fingerprinted_and_setuid_files_are_refused() {
    let mut f = fixture();
    let exe = f.candidate.join("Contents/MacOS/octosense");
    let before = digest_tree(&f.candidate).unwrap();
    for mode in [0o1755, 0o4755] {
        fs::set_permissions(&exe, fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(fs::metadata(&exe).unwrap().mode() & 0o7777, mode);
        assert_ne!(digest_tree(&f.candidate).unwrap(), before, "{mode:o}");
    }
    assert!(validate_bundle(&f.candidate, Some("0.1.0-rc.6"))
        .unwrap_err()
        .contains("setuid"));
    // Even a job prepared with that exact fingerprint cannot install it.
    f.prepare_again();
    let error = install_committed(&f.ticket, &f.job, &f.candidate, &fixtures, QUICK).unwrap_err();
    assert!(error.contains("setuid"), "{error}");
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
    assert_eq!(digest_tree(&f.candidate).unwrap(), f.ticket.new_digest);
}

// Downgrades: the helper itself refuses an update that is not newer.
#[test]
fn an_update_that_is_not_newer_is_refused_before_the_exchange() {
    for version in ["0.1.0-rc.4", "0.1.0-rc.5"] {
        let mut f = fixture();
        bundle(&f.ticket.target, "0.1.0-rc.5", OPENED);
        bundle(&f.candidate, version, NEVER);
        f.ticket.new_tag = format!("desktop-v{version}");
        f.prepare_again();
        let error =
            install_committed(&f.ticket, &f.job, &f.candidate, &fixtures, QUICK).unwrap_err();
        assert!(error.contains("must be newer"), "{version}: {error}");
        assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
        assert_eq!(wait_for(&f.profile().join("opened")), "rolled-back");
        assert!(!f.profile().join("new-started").exists());
    }
    // A job without a release tag cannot skip the comparison.
    let mut f = fixture();
    f.ticket.new_tag = "0.1.0-rc.6".into();
    f.prepare_again();
    assert!(
        verify_before_exchange(&f.ticket, &f.candidate, &Trust::Fixtures, QUICK)
            .unwrap_err()
            .contains("release tag")
    );
    // Semantic order, not text order: rc.10 follows rc.9.
    let mut f = fixture();
    bundle(&f.ticket.target, "0.1.0-rc.9", "#!/bin/sh\n");
    bundle(&f.candidate, "0.1.0-rc.10", "#!/bin/sh\n");
    f.ticket.new_tag = "desktop-v0.1.0-rc.10".into();
    f.prepare_again();
    verify_before_exchange(&f.ticket, &f.candidate, &Trust::Fixtures, QUICK).unwrap();
    swap_in(&f.ticket, f.root(), &f.candidate).unwrap();
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.new_digest);
}

// `--restore` swaps back only the exact previous OctoSense bundle.
#[test]
fn restore_puts_back_only_the_exact_previous_octosense_bundle() {
    let restore = |f: &Fixture| {
        restore_previous(
            &f.ticket,
            &f.job,
            &f.candidate,
            &Trust::Fixtures,
            "Restored.",
        )
    };
    // An installation's honest leftovers are restored.
    let f = fixture();
    exchange(&f.ticket.target, &f.candidate).unwrap();
    restore(&f).unwrap();
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
    // A planted job is refused even with fingerprints recorded to match.
    type Plant = (&'static str, fn(&Fixture));
    let plants: [Plant; 3] = [
        ("retained app with another bundle id", |f| {
            let plist = f.candidate.join("Contents/Info.plist");
            let text = fs::read_to_string(&plist).unwrap();
            fs::write(&plist, text.replace(BUNDLE_ID, "org.example.other")).unwrap();
        }),
        ("retained app with a setuid file", |f| {
            let exe = f.candidate.join("Contents/MacOS/octosense");
            fs::set_permissions(exe, fs::Permissions::from_mode(0o4755)).unwrap();
        }),
        ("installed app without its executable", |f| {
            fs::remove_file(f.ticket.target.join("Contents/MacOS/octosense")).unwrap();
        }),
    ];
    for (case, plant) in plants {
        let mut f = fixture();
        exchange(&f.ticket.target, &f.candidate).unwrap();
        plant(&f);
        f.ticket.old_digest = digest_tree(&f.candidate).unwrap();
        f.ticket.new_digest = digest_tree(&f.ticket.target).unwrap();
        let error = restore(&f).unwrap_err();
        assert!(
            error.contains("not an OctoSense app bundle"),
            "{case}: {error}"
        );
        assert_eq!(
            digest_tree(&f.ticket.target).unwrap(),
            f.ticket.new_digest,
            "{case}"
        );
    }
    // Nor anything but the exact bundle this installation replaced.
    let f = fixture();
    exchange(&f.ticket.target, &f.candidate).unwrap();
    fs::write(f.candidate.join("Contents/extra"), b"not the replaced app").unwrap();
    assert!(restore(&f).unwrap_err().contains("changed"));
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.new_digest);
}

// A changed bundle that something keeps running is still swapped out.
#[test]
fn a_changed_bundle_kept_running_is_still_swapped_out() {
    let mut f = fixture();
    bundle(&f.ticket.target, "0.1.0-rc.5", OPENED);
    ad_hoc_sign(&f.ticket.target, BUNDLE_ID);
    bundle(&f.candidate, "0.1.0-rc.6", NEVER);
    f.prepare_again();
    let trust = codesign::Identity::ad_hoc_fixture(&f.ticket.target)
        .map(Trust::Signed)
        .unwrap();
    // After the exchange the installed bundle is not the checked one, and a
    // process runs from inside it.
    exchange(&f.ticket.target, &f.candidate).unwrap();
    let mut running = sidecar(&f.ticket.target, 30_000);
    assert!(ensure_bundle_stopped(&f.ticket.target).is_err());
    finish_exchanged(&f.ticket, &f.job, &f.candidate, &trust, QUICK).unwrap();
    let kept_running = running.try_wait().unwrap().is_none();
    let _ = running.kill();
    let _ = running.wait();
    assert!(
        kept_running,
        "the fixture process should keep running throughout"
    );
    // Swapped out at once, and the previous app reopened.
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
    assert!(f.candidate.join("Contents/MacOS/octos-kernel").is_file());
    assert_eq!(wait_for(&f.profile().join("opened")), "rolled-back");
    assert!(!f.profile().join("new-started").exists());
    assert_eq!(f.receipt()["state"], "rolled_back");
}

/// Another macOS user's installer of the same app, with its own staging
/// folder, receipt folder and update (the same release, built differently).
fn second_job(f: &Fixture, program: &str) -> (Ticket, PathBuf, PathBuf) {
    let parent = f.ticket.target.parent().unwrap();
    let nonce = uuid::Uuid::new_v4().to_string();
    let root = parent.join(format!(".octosense-update-{nonce}"));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let candidate = root.join("candidate.app");
    bundle(&candidate, "0.1.0-rc.6", program);
    let updates = parent.join(format!("updates-{nonce}"));
    fs::DirBuilder::new().mode(0o700).create(&updates).unwrap();
    let ticket = Ticket {
        protocol: 1,
        nonce,
        parent_pid: f.ticket.parent_pid,
        target: f.ticket.target.clone(),
        old_digest: digest_tree(&f.ticket.target).unwrap(),
        new_digest: digest_tree(&candidate).unwrap(),
        new_tag: f.ticket.new_tag.clone(),
        environment: f.ticket.environment.clone(),
        startup_ack: true,
        receipt: updates.join("last-install.json"),
    };
    let job = root.join("job.json");
    write_json(&job, &ticket).unwrap();
    (ticket, job, candidate)
}

// An installer the lock does not cover can swap at the same moment. The one
// that swaps second sees that what came out is not the app its job recorded,
// and puts the first one's update back.
#[test]
fn an_installer_that_swaps_second_puts_the_first_ones_update_back() {
    let a = fixture();
    let (b, b_job, b_candidate) = second_job(&a, "#!/bin/sh\nexit 3\n");
    let b_root = b_job.parent().unwrap();
    // Both pass their checks while the previous app is still installed...
    verify_before_exchange(&a.ticket, &a.candidate, &Trust::Fixtures, QUICK).unwrap();
    verify_before_exchange(&b, &b_candidate, &Trust::Fixtures, QUICK).unwrap();
    // ...then A swaps, and B a moment later.
    swap_in(&a.ticket, a.root(), &a.candidate).unwrap();
    let error = swap_in(&b, b_root, &b_candidate).unwrap_err();
    assert!(error.contains("Another installation"), "{error}");
    // Every bundle is where its job says: A's update is installed with the
    // previous app as its backup, and B's update sits unused in its folder.
    assert_eq!(digest_tree(&a.ticket.target).unwrap(), a.ticket.new_digest);
    assert_eq!(digest_tree(&a.candidate).unwrap(), a.ticket.old_digest);
    assert_eq!(digest_tree(&b_candidate).unwrap(), b.new_digest);
    // So the sweep keeps A's backup, which `--restore` needs, and later
    // removes B's unused update.
    phase(a.root(), "complete").unwrap();
    phase(b_root, "not_installed").unwrap();
    let lock = InstallLock::acquire_in(&lock_folder(&a), &a.ticket.target).unwrap();
    let later = SystemTime::now() + 2 * DISPOSABLE_AFTER;
    assert_eq!(sweep_retained(&a.ticket.target, &lock, later), 1);
    assert!(a.root().is_dir() && !b_root.exists());
}

// A new app that started and then failed is swapped out at once as well,
// even while a process it left keeps running from its bundle.
#[test]
fn a_new_app_that_started_and_failed_is_swapped_out_while_its_process_runs() {
    let mut f = fixture();
    bundle(&f.ticket.target, "0.1.0-rc.5", OPENED);
    // The update starts and exits before its first frame, and a process
    // runs from its bundle throughout, as a kernel sidecar could.
    bundle(&f.candidate, "0.1.0-rc.6", "#!/bin/sh\nexit 7\n");
    let mut lingering = sidecar(&f.candidate, 30_000);
    f.prepare_again();
    install_committed(&f.ticket, &f.job, &f.candidate, &fixtures, QUICK).unwrap();
    let kept_running = lingering.try_wait().unwrap().is_none();
    let _ = lingering.kill();
    let _ = lingering.wait();
    assert!(
        kept_running,
        "the fixture process should keep running throughout"
    );
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
    assert_eq!(digest_tree(&f.candidate).unwrap(), f.ticket.new_digest);
    assert_eq!(wait_for(&f.profile().join("opened")), "rolled-back");
    assert_eq!(f.receipt()["state"], "rolled_back");
    assert!(f.receipt()["message"]
        .as_str()
        .unwrap()
        .contains("exited before startup"));
}

// Versions come from the Info.plist the signature check validated, not from
// the file as it reads afterwards.
#[test]
fn versions_come_from_the_validated_signature_not_a_later_read() {
    let f = fixture();
    ad_hoc_sign(&f.ticket.target, BUNDLE_ID);
    let trust = codesign::Identity::ad_hoc_fixture(&f.ticket.target)
        .map(Trust::Signed)
        .unwrap();
    assert_eq!(trust.admit(&f.ticket.target).unwrap(), "0.1.0-rc.5");
    // The installed app's file now claims another version: the sealed one
    // stands, and the changed file no longer passes the signature check.
    let plist = f.ticket.target.join("Contents/Info.plist");
    let text = fs::read_to_string(&plist).unwrap();
    fs::write(&plist, text.replace("0.1.0-rc.5", "9.9.9")).unwrap();
    assert_eq!(
        plist_field(&f.ticket.target, "CFBundleShortVersionString").unwrap(),
        "9.9.9"
    );
    assert_eq!(
        trust.installed_version(&f.ticket.target).unwrap(),
        "0.1.0-rc.5"
    );
    assert!(trust.admit(&f.ticket.target).is_err());
}

// The receipt says rolled back only when the previous app is started again;
// when that fails, it says what happened instead.
#[test]
fn the_receipt_says_rolled_back_only_when_the_previous_app_reopens() {
    let mut f = fixture();
    bundle(&f.ticket.target, "0.1.0-rc.5", OPENED);
    f.prepare_again();
    // Nothing can be started: the relaunch log is a link, which the helper
    // refuses to open.
    let log = f
        .ticket
        .receipt
        .parent()
        .unwrap()
        .join(format!("relaunch-{}.log", f.ticket.nonce));
    std::os::unix::fs::symlink("/dev/null", &log).unwrap();
    let error = install_committed(&f.ticket, &f.job, &f.candidate, &fixtures, QUICK).unwrap_err();
    assert!(error.contains("could not be reopened"), "{error}");
    // The previous app was restored, and the receipt claims no reopening.
    assert_eq!(digest_tree(&f.ticket.target).unwrap(), f.ticket.old_digest);
    assert!(f.journal().contains("rolled_back"));
    assert_eq!(f.receipt()["state"], "failed");
    assert!(f.receipt()["message"]
        .as_str()
        .unwrap()
        .contains("could not be reopened"));
    assert!(!f.profile().join("opened").exists());
}
