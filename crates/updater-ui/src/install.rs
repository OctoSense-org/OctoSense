//! Handoff to the operating system after download verification. This module
//! never runs a shell, replaces the running app, or reports that an opened
//! installer has completed installation.

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
            message: "The verified package was handed to your desktop's package installer. Review and install it there, then restart OctoSense. If no installer opens, use your distribution's package manager on the downloaded file.",
        }),
        Platform::LinuxX64AppImage if has_extension("AppImage") => Ok(InstallPlan {
            program: "xdg-open".into(),
            args: vec![path.parent().ok_or("The update folder is unavailable.")?.as_os_str().into()],
            wait: true,
            message: "The folder containing the verified AppImage is open. Quit OctoSense, replace your old AppImage with this file, allow execution in its file properties, and reopen it. The running app has not been replaced.",
        }),
        Platform::AndroidArm64 => Err("Android updates use the host's package installer.".into()),
        _ => Err("This file is not an installer for the selected platform.".into()),
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
