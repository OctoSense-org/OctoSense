use super::*;

struct Staging(PathBuf, bool);
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.1 {
            return;
        }
        let mount = self.0.join("mount");
        if mount.exists() {
            let _ = run(
                Command::new("/usr/bin/hdiutil").arg("detach").arg(&mount),
                &Cancellation::default(),
                "release the disk image",
            );
        }
        // Never walk a mounted image if detaching failed.
        if mount
            .read_dir()
            .is_ok_and(|mut entries| entries.next().is_some())
        {
            return;
        }
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(command: &mut Command, cancellation: &Cancellation, action: &str) -> Result<()> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not {action}: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        if cancellation.is_cancelled() || Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("Cancelled or timed out while trying to {action}."));
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "Could not {action} ({status}). Use Open installer instead."
                ))
            };
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Revalidates the signed download, extracts it privately, validates the app,
/// and starts a helper which cannot replace anything until the shell commits.
pub fn prepare(artifact: &VerifiedArtifact, cancellation: &Cancellation) -> Result<Prepared> {
    if artifact.platform() != Platform::MacArm64 {
        return Err("Not a macOS update.".into());
    }
    let dmg = artifact.revalidate().map_err(|e| e.to_string())?;
    let target = current_bundle()?;
    validate_bundle(&target, None)?;
    // Only code signed with this installation's own identity may replace it.
    // An unsigned source build is told so before anything is staged; the
    // helper checks again, independently, before the exchange and launch.
    let trust = Trust::of_installed(&target)?;
    let version = artifact
        .tag()
        .strip_prefix("desktop-v")
        .ok_or("Invalid release tag.")?;
    if semver::Version::parse(version).map_err(|e| e.to_string())?
        <= semver::Version::parse(&plist_field(&target, "CFBundleShortVersionString")?)
            .map_err(|e| e.to_string())?
    {
        return Err("The update must be newer than the installed app.".into());
    }
    let parent = target.parent().ok_or("No installation folder.")?;
    let nonce = uuid::Uuid::new_v4().to_string();
    let root = parent.join(format!(".octosense-update-{nonce}"));
    fs::DirBuilder::new().mode(0o700).create(&root)
        .map_err(|_| "The installation folder is not writable. Use Open installer, or move OctoSense to your own Applications folder.".to_string())?;
    let mut staging = Staging(root.clone(), true);
    let mount = root.join("mount");
    fs::create_dir(&mount).map_err(|e| e.to_string())?;
    run(
        Command::new("/usr/bin/hdiutil")
            .args([
                "attach",
                "-readonly",
                "-nobrowse",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(&mount)
            .arg(&dmg),
        cancellation,
        "open the verified disk image",
    )?;
    let from = mount.join("OctoSense.app");
    validate_bundle(&from, Some(version))?;
    let new_digest = digest_tree(&from)?;
    let candidate = root.join("candidate.app");
    run(
        Command::new("/usr/bin/ditto").arg(&from).arg(&candidate),
        cancellation,
        "stage the new app",
    )?;
    if digest_tree(&candidate)? != new_digest {
        return Err("The staged app differs from the verified disk image.".into());
    }
    trust.admit(&candidate).map_err(|reason| {
        format!("The downloaded app {reason}, so Install & Restart refused it. Use Open installer manually.")
    })?;
    run(
        Command::new("/usr/bin/hdiutil").arg("detach").arg(&mount),
        cancellation,
        "close the disk image",
    )?;
    fs::remove_dir(&mount).map_err(|e| e.to_string())?;
    artifact.revalidate().map_err(|e| e.to_string())?;
    let cache = dmg
        .parent()
        .and_then(Path::parent)
        .ok_or("No private update folder.")?;
    private_dir(cache)?;
    let helper = root.join(HELPER);
    let original_helper = target.join("Contents/MacOS").join(HELPER);
    if !fs::symlink_metadata(&original_helper).is_ok_and(|m| m.is_file()) {
        return Err("The installation has no update helper. Use Open installer instead.".into());
    }
    fs::copy(original_helper, &helper).map_err(|e| e.to_string())?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    let startup_ack = fs::read(candidate.join("Contents/Resources/octosense-updater.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .is_some_and(|v| v["protocol"] == 1);
    let ticket = Ticket {
        protocol: 1,
        nonce: nonce.clone(),
        parent_pid: std::process::id(),
        target: target.clone(),
        old_digest: digest_tree(&target)?,
        new_digest,
        new_tag: artifact.tag().into(),
        environment: environment_keys()
            .into_iter()
            .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
            .collect(),
        startup_ack,
        receipt: cache.join("last-install.json"),
    };
    let job = root.join("job.json");
    write_json(&job, &ticket)?;
    if cancellation.is_cancelled() {
        return Err("Update cancelled before shutdown.".into());
    }
    let log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(cache.join(format!("install-{nonce}.log")))
        .map_err(|e| e.to_string())?;
    let mut command = Command::new(&helper);
    command.arg(&job).env_clear().env("PATH", "/usr/bin:/bin");
    #[cfg(debug_assertions)]
    if let Some(value) = std::env::var_os(UNSIGNED_FIXTURES_ENV) {
        command.env(UNSIGNED_FIXTURES_ENV, value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(log)
        .spawn()
        .map_err(|e| e.to_string())?;
    let input = child.stdin.take();
    let stdout = child.stdout.take().ok_or("Missing helper handshake.")?;
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut response = String::new();
        let result = BufReader::new(stdout)
            .take(64)
            .read_line(&mut response)
            .map(|_| response);
        let _ = send.send(result);
    });
    match receive.recv_timeout(Duration::from_secs(20)) {
        Ok(Ok(answer)) if answer.trim() == "ready" && !cancellation.is_cancelled() => {
            staging.1 = false;
            Ok(Prepared {
                child: Some(child),
                input,
                nonce,
            })
        }
        _ => {
            drop(input);
            let _ = child.kill();
            let _ = child.wait();
            Err("The update helper could not start. The installed app was not changed.".into())
        }
    }
}
