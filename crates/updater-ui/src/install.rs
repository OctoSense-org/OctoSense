//! Handoff to the operating system after download verification. This module
//! never runs a shell, replaces the running app, or reports that an opened
//! installer has completed installation.
//!
//! Linux packages are handed over from the person's Downloads folder, where a
//! browser would have saved them: a desktop package handler may be unable to
//! read the hidden profile folder the download lives in (Ubuntu's App Center
//! is a snap, and snaps cannot see hidden folders), and someone replacing an
//! AppImage must find the new one.

use octosense_updater::{Cancellation, Platform, VerifiedArtifact};
use std::{
    ffi::OsString,
    path::Path,
    process::{Child, Command},
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq, Eq)]
struct InstallPlan {
    program: OsString,
    args: Vec<OsString>,
    // A Windows installer stays alive while the user reviews its prompts.
    wait: bool,
    message: &'static str,
}

fn plan(platform: Platform, path: &Path) -> Result<InstallPlan, String> {
    if !path.is_absolute() {
        return Err("The verified update must have an absolute file path.".into());
    }
    let has_extension = |expected: &str| {
        path.extension().and_then(|extension| extension.to_str()) == Some(expected)
    };
    match platform {
        Platform::MacArm64 if has_extension("dmg") => Ok(InstallPlan {
            program: "/usr/bin/open".into(),
            args: vec![path.as_os_str().into()],
            wait: true,
            message: "The verified disk image was handed to macOS. Quit OctoSense, then drag OctoSense to Applications and confirm replacement. Your app data stays in its existing profile.",
        }),
        Platform::WindowsX64 if has_extension("exe") => Ok(InstallPlan {
            program: path.as_os_str().into(),
            args: vec![],
            wait: false,
            message: "The verified installer is open. Follow its prompts and close OctoSense when requested. Installation is not complete until the installer finishes.",
        }),
        Platform::LinuxX64Deb if has_extension("deb") => Ok(InstallPlan {
            program: "xdg-open".into(),
            args: vec![path.as_os_str().into()],
            wait: true,
            message: "The verified package, saved in your Downloads folder, was handed to your desktop's package installer. Review and install it there, then restart OctoSense. If no installer opens, install that file with your distribution's package manager.",
        }),
        Platform::LinuxX64AppImage if has_extension("AppImage") => Ok(InstallPlan {
            program: "xdg-open".into(),
            args: vec![path.parent().ok_or("The update folder is unavailable.")?.as_os_str().into()],
            wait: true,
            message: "The verified AppImage is in your Downloads folder, ready to run. Quit OctoSense, replace your old AppImage with it, then open it. The running app has not been replaced.",
        }),
        Platform::AndroidArm64 => Err("Android updates use the host's package installer.".into()),
        _ => Err("This file is not an installer for the selected platform.".into()),
    }
}

/// The person's Downloads folder, and a verified copy of a download in it.
#[cfg(any(target_os = "linux", test))]
mod downloads {
    use sha2::{Digest, Sha256};
    use std::{
        fs::{self, File, OpenOptions},
        io::{Read, Write},
        path::{Path, PathBuf},
    };

    /// `XDG_DOWNLOAD_DIR` from `user-dirs.dirs`, else `~/Downloads`, else home.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(super) fn folder() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())?;
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let configured = fs::read_to_string(config.join("user-dirs.dirs"))
            .ok()
            .and_then(|text| configured_folder(&text, &home));
        [configured, Some(home.join("Downloads")), Some(home)]
            .into_iter()
            .flatten()
            .find(|path| path.is_dir())
    }

    /// The `XDG_DOWNLOAD_DIR="$HOME/…"` (or absolute) line of `user-dirs.dirs`.
    pub(super) fn configured_folder(text: &str, home: &Path) -> Option<PathBuf> {
        let value = text
            .lines()
            .map(str::trim)
            .find_map(|line| line.strip_prefix("XDG_DOWNLOAD_DIR="))?
            .strip_prefix('"')?
            .strip_suffix('"')?;
        let path = match value.strip_prefix("$HOME") {
            Some("") => home.to_path_buf(),
            Some(rest) => home.join(rest.strip_prefix('/')?),
            None => PathBuf::from(value),
        };
        (path.is_absolute() && !value[1..].contains('$')).then_some(path)
    }

    /// Copy `source` into `folder` under a name nothing there uses (or reuse an
    /// identical file), checking the copy's bytes against `sha256`.
    pub(super) fn stage(
        source: &Path,
        sha256: &str,
        folder: &Path,
        executable: bool,
    ) -> Result<PathBuf, String> {
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("The verified update has no file name.")?;
        let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
        for n in 1..100 {
            let candidate = folder.join(if n == 1 {
                name.to_string()
            } else {
                format!("{stem} ({n}).{extension}")
            });
            match create(&candidate, executable) {
                Ok(file) => return copy_verified(source, file, &candidate, sha256),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if is_regular(&candidate) && digest(&candidate).as_deref() == Some(sha256) {
                        mark_executable(&candidate, executable)?;
                        return Ok(candidate);
                    }
                }
                Err(_) => return Err("Could not save the update in your Downloads folder.".into()),
            }
        }
        Err("Your Downloads folder already holds too many files with this update's name.".into())
    }

    fn create(path: &Path, executable: bool) -> std::io::Result<File> {
        let mut options = OpenOptions::new();
        // Never follows or replaces an existing file or link.
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(if executable { 0o755 } else { 0o644 });
        }
        #[cfg(not(unix))]
        let _ = executable;
        options.open(path)
    }

    fn copy_verified(
        source: &Path,
        mut file: File,
        path: &Path,
        sha256: &str,
    ) -> Result<PathBuf, String> {
        let result = (|| {
            let mut input = File::open(source).ok()?;
            let mut hasher = Sha256::new();
            let mut buffer = vec![0u8; 1024 * 1024];
            loop {
                let read = input.read(&mut buffer).ok()?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                file.write_all(&buffer[..read]).ok()?;
            }
            file.sync_all().ok()?;
            (format!("{:x}", hasher.finalize()) == sha256).then_some(())
        })();
        if result.is_none() {
            let _ = fs::remove_file(path);
            return Err("The update could not be saved in your Downloads folder unchanged.".into());
        }
        Ok(path.to_path_buf())
    }

    fn is_regular(path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file())
    }

    fn digest(path: &Path) -> Option<String> {
        let mut input = File::open(path).ok()?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        loop {
            let read = input.read(&mut buffer).ok()?;
            if read == 0 {
                return Some(format!("{:x}", hasher.finalize()));
            }
            hasher.update(&buffer[..read]);
        }
    }

    fn mark_executable(path: &Path, executable: bool) -> Result<(), String> {
        #[cfg(unix)]
        if executable {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .map_err(|_| "Could not make the AppImage executable.".to_string())?;
        }
        #[cfg(not(unix))]
        let _ = (path, executable);
        Ok(())
    }
}

fn host_supports(platform: Platform) -> bool {
    match platform {
        Platform::MacArm64 => cfg!(all(target_os = "macos", target_arch = "aarch64")),
        Platform::WindowsX64 => cfg!(all(target_os = "windows", target_arch = "x86_64")),
        Platform::LinuxX64Deb | Platform::LinuxX64AppImage => {
            cfg!(all(target_os = "linux", target_arch = "x86_64"))
        }
        Platform::AndroidArm64 => false,
    }
}

/// Called only after a person requests installation, on the updater worker.
/// Recheck the full downloaded file immediately before opening the installer.
pub fn install(artifact: &VerifiedArtifact, cancellation: &Cancellation) -> Result<String, String> {
    if !host_supports(artifact.platform()) {
        return Err("This update cannot be installed on the running platform.".into());
    }
    let path = artifact.revalidate().map_err(|error| error.to_string())?;
    #[cfg(target_os = "linux")]
    let path = match artifact.platform() {
        Platform::LinuxX64Deb | Platform::LinuxX64AppImage => {
            if cancellation.is_cancelled() {
                return Err(
                    "Update installation was cancelled before opening the installer.".into(),
                );
            }
            let downloads = downloads::folder().ok_or("Your Downloads folder is unavailable.")?;
            let executable = artifact.platform() == Platform::LinuxX64AppImage;
            downloads::stage(&path, artifact.sha256(), &downloads, executable)?
        }
        _ => path,
    };
    let plan = plan(artifact.platform(), &path)?;
    // Hashing a large installer takes time. A close/cancel during verification
    // must not leave a deferred OS installer prompt behind.
    if cancellation.is_cancelled() {
        return Err("Update installation was cancelled before opening the installer.".into());
    }
    let child = Command::new(&plan.program)
        .args(&plan.args)
        .spawn()
        .map_err(|error| format!("Could not open the verified installer: {error}"))?;
    observe_handoff(
        child,
        if plan.wait {
            Duration::from_secs(2)
        } else {
            Duration::ZERO
        },
    )?;
    Ok(plan.message.into())
}

// Some desktop handlers keep their launcher alive until the installer closes.
// Once spawned, the OS owns the interaction: preserve its source and release
// the updater worker promptly, rather than holding global BUSY indefinitely.
fn observe_handoff(mut child: Child, maximum: Duration) -> Result<(), String> {
    let deadline = Instant::now() + maximum;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("The operating system could not open the verified update ({status}). The file remains downloaded.")),
            Ok(None) => {}
            Err(_) => break,
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn long_lived_handler_does_not_hold_the_updater_worker() {
        let child = Command::new("/bin/sleep").arg("1").spawn().unwrap();
        let started = Instant::now();
        observe_handoff(child, Duration::from_millis(20)).unwrap();
        assert!(started.elapsed() < Duration::from_millis(750));
    }

    #[cfg(unix)]
    #[test]
    fn immediate_handler_failure_is_reported() {
        let child = Command::new("/usr/bin/false").spawn().unwrap();
        assert!(observe_handoff(child, Duration::from_secs(1)).is_err());
    }

    #[test]
    fn mac_disk_image_is_one_literal_argument() {
        let path = std::env::temp_dir().join("OctoSense $(touch bad); quoted ' name.dmg");
        let command = plan(Platform::MacArm64, &path).unwrap();
        assert_eq!(command.program, "/usr/bin/open");
        assert_eq!(command.args, vec![path.as_os_str()]);
        assert!(command.wait);
    }

    #[test]
    fn windows_setup_is_executed_directly_without_a_shell() {
        // An absolute path for the platform running this test. Command gets
        // the verified file as its program, never `cmd /c` or `start`.
        let path = std::env::temp_dir().join("setup with & special characters.exe");
        let command = plan(Platform::WindowsX64, &path).unwrap();
        assert_eq!(command.program, path.as_os_str());
        assert!(command.args.is_empty());
        assert!(!command.wait);
    }

    #[test]
    fn linux_handoffs_do_not_execute_the_download_or_replace_the_running_file() {
        let directory = std::env::temp_dir();
        let deb_path = directory.join("new.deb");
        let deb = plan(Platform::LinuxX64Deb, &deb_path).unwrap();
        assert_eq!(deb.program, "xdg-open");
        assert_eq!(deb.args, vec![deb_path.as_os_str()]);
        let image_path = directory.join("new.AppImage");
        let image = plan(Platform::LinuxX64AppImage, &image_path).unwrap();
        assert_eq!(image.program, "xdg-open");
        assert_eq!(image.args, vec![image_path.parent().unwrap().as_os_str()]);
    }

    #[test]
    fn the_downloads_folder_comes_from_user_dirs() {
        let home = Path::new("/home/person");
        for (text, expected) in [
            (
                "XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"",
                Some("/home/person/Downloads"),
            ),
            (
                "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\n  XDG_DOWNLOAD_DIR=\"$HOME/下载\"",
                Some("/home/person/下载"),
            ),
            ("XDG_DOWNLOAD_DIR=\"$HOME\"", Some("/home/person")),
            (
                "XDG_DOWNLOAD_DIR=\"/data/downloads\"",
                Some("/data/downloads"),
            ),
            ("XDG_DOWNLOAD_DIR=\"Downloads\"", None),
            ("XDG_DOWNLOAD_DIR=\"$HOME/$OTHER\"", None),
            ("XDG_DOWNLOAD_DIR=$HOME/Downloads", None),
            ("XDG_DOCUMENTS_DIR=\"$HOME/Documents\"", None),
        ] {
            assert_eq!(
                downloads::configured_folder(text, home),
                expected.map(std::path::PathBuf::from),
                "{text}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn linux_packages_are_copied_to_downloads_unchanged_without_replacing_anything() {
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::PermissionsExt;
        let cache = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let source = cache.path().join("octosense_0.2.0_x86_64.AppImage");
        std::fs::write(&source, b"new image").unwrap();
        let sha = format!("{:x}", Sha256::digest(b"new image"));
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;

        // A person's own file with that name stays; the copy takes the next name.
        let theirs = folder.path().join("octosense_0.2.0_x86_64.AppImage");
        std::fs::write(&theirs, b"their file").unwrap();
        let staged = downloads::stage(&source, &sha, folder.path(), true).unwrap();
        assert_eq!(
            staged,
            folder.path().join("octosense_0.2.0_x86_64 (2).AppImage")
        );
        assert_eq!(std::fs::read(&staged).unwrap(), b"new image");
        assert_eq!(std::fs::read(&theirs).unwrap(), b"their file");
        assert_eq!(mode(&staged), 0o755);
        // Staging again reuses the identical copy.
        assert_eq!(
            downloads::stage(&source, &sha, folder.path(), true).unwrap(),
            staged
        );

        // A link is never followed or replaced.
        let elsewhere = tempfile::tempdir().unwrap();
        let target = elsewhere.path().join("target");
        std::fs::write(&target, b"untouched").unwrap();
        let deb = cache.path().join("octosense_0.2.0_amd64.deb");
        std::fs::write(&deb, b"package").unwrap();
        std::os::unix::fs::symlink(&target, folder.path().join("octosense_0.2.0_amd64.deb"))
            .unwrap();
        let deb_sha = format!("{:x}", Sha256::digest(b"package"));
        let staged = downloads::stage(&deb, &deb_sha, folder.path(), false).unwrap();
        assert_eq!(staged, folder.path().join("octosense_0.2.0_amd64 (2).deb"));
        assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
        assert_eq!(mode(&staged), 0o644);

        // A copy that does not match the verified digest is removed.
        let wrong = "0".repeat(64);
        assert!(downloads::stage(&deb, &wrong, elsewhere.path(), false).is_err());
        assert!(!elsewhere.path().join("octosense_0.2.0_amd64.deb").exists());
    }

    #[test]
    fn wrong_extensions_relative_paths_and_android_are_refused() {
        assert!(plan(Platform::MacArm64, Path::new("relative.dmg")).is_err());
        for (platform, filename) in [
            (Platform::MacArm64, "setup.exe"),
            (Platform::WindowsX64, "setup.cmd"),
            (Platform::LinuxX64Deb, "new.AppImage"),
            (Platform::LinuxX64AppImage, "new.deb"),
            (Platform::AndroidArm64, "home.apk"),
        ] {
            assert!(
                plan(platform, &std::env::temp_dir().join(filename)).is_err(),
                "{filename}"
            );
        }
    }
}
