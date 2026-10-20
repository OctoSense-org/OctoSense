//! Background builds of the catalog's process apps, for a shell run from a
//! checkout.
//!
//! From a checkout, a process app's first open runs its release build
//! ([`clients::launch_plan`]) before the app starts: seconds for a small
//! app, a minute or two cold (Makepad's pdf app, 120 crates, about 100 s on
//! an M-series Mac; route 110 s more on top). The prebuild runs those same
//! builds once in the background, shortly after the shell starts, so a
//! later open finds its binary built and starts at once.
//!
//! - One build at a time, in catalog order, each the exact command a launch
//!   would run ([`clients::build_command`]), at background priority
//!   (darwinbg on macOS, nice 19 elsewhere): it never competes with the
//!   person's apps.
//! - It steps aside for a launch: while any launch is building (the WM
//!   counts them every tick, [`set_launches_building`]), it stops its own
//!   cargo and waits, then starts that build again; cargo keeps what it
//!   finished.
//! - Cargo returns at once for an app that is already built, so a later
//!   start costs a few seconds of up-to-date checks. Each success is
//!   recorded beside the binary ([`clients::note_built`]): an open then
//!   starts that binary without cargo while nothing its build read has
//!   changed ([`clients::build_is_fresh`]), and this run of the same build
//!   catches what that comparison cannot see.
//! - Quitting the shell stops it ([`shutdown`]): its cargo has a process
//!   group of its own, so nothing else would end it.
//! - An installed shell has no build to run and never starts it, and
//!   `MAKEPAD_WM_NO_PREBUILD` (any value but empty or `0`) turns it off.
use crate::clients::{self, AppDef, BuildStep};
use makepad_widgets::makepad_platform::thread::{ThreadOptions, ThreadSpawner};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// How long after startup the first build waits: the shell's own start,
/// and whatever the person opens first, come before it.
const START_DELAY: Duration = Duration::from_secs(20);

/// How often a running build checks whether a launch needs the machine.
const POLL: Duration = Duration::from_millis(500);

/// Launches whose build is running, as the WM last counted them.
static LAUNCHES_BUILDING: AtomicUsize = AtomicUsize::new(0);

/// The WM's count of launches whose build is running, every tick.
pub fn set_launches_building(count: usize) {
    LAUNCHES_BUILDING.store(count, Ordering::Relaxed);
}

fn launch_is_building() -> bool {
    LAUNCHES_BUILDING.load(Ordering::Relaxed) > 0
}

/// Set once the shell quits: no further build starts.
static STOPPED: AtomicBool = AtomicBool::new(false);

/// The running build's process id (its group's), 0 when none runs.
static RUNNING: AtomicU32 = AtomicU32::new(0);

/// The shell is quitting: stop the running build, its whole group, and start
/// no other. Called on `Event::Shutdown`; only atomics, nothing to wait on.
pub fn shutdown() {
    STOPPED.store(true, Ordering::SeqCst);
    let pid = RUNNING.swap(0, Ordering::SeqCst);
    #[cfg(unix)]
    if pid != 0 {
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGTERM);
        }
    }
    #[cfg(not(unix))]
    let _ = pid; // the worker stops its child when it sees STOPPED
}

/// `MAKEPAD_WM_NO_PREBUILD` turns the prebuild off. An empty or `0` value is
/// not a request, as for `MAKEPAD_WM_NO_WARM`.
pub fn enabled(no_prebuild: Option<&str>) -> bool {
    match no_prebuild {
        None => true,
        Some(v) => matches!(v.trim(), "" | "0"),
    }
}

/// The builds to run: the launch build of every listed catalog app that has
/// one, each identical build once, in catalog order. Empty for an installed
/// shell (no checkout, so no app has a build).
pub fn plan(apps: &[AppDef], root: Option<&Path>) -> Vec<(String, BuildStep)> {
    plan_listed(apps, root, crate::apps::listed)
}

fn plan_listed(apps: &[AppDef], root: Option<&Path>, listed: impl Fn(&str) -> bool) -> Vec<(String, BuildStep)> {
    let mut out: Vec<(String, BuildStep)> = Vec::new();
    for app in apps.iter().filter(|app| listed(&app.id)) {
        let Ok(launch) = clients::launch_plan(app, root, &[]) else {
            continue;
        };
        let Some(build) = launch.build else {
            continue;
        };
        if out.iter().any(|(_, have)| have.dir == build.dir && have.args == build.args) {
            continue;
        }
        out.push((app.id.clone(), build));
    }
    out
}

/// Start the prebuild on a worker of its own, when this shell runs from a
/// checkout and nothing turned it off.
pub fn start(spawner: &ThreadSpawner) {
    if !crate::host::processes_available() || !enabled(std::env::var("MAKEPAD_WM_NO_PREBUILD").ok().as_deref()) {
        return;
    }
    let steps = plan(&clients::available_apps(), clients::repo_root().as_deref());
    if steps.is_empty() {
        return;
    }
    let ids: Vec<&str> = steps.iter().map(|(id, _)| id.as_str()).collect();
    makepad_widgets::log!("prebuild: {} catalog apps build in the background: {}", steps.len(), ids.join(", "));
    let submitted = spawner.spawn_worker(
        ThreadOptions { name: Some("wm-prebuild".into()), ..Default::default() },
        move || run(steps),
    );
    match submitted {
        Ok(task) => task.detach(),
        Err(error) => makepad_widgets::log!("prebuild: could not start: {error}"),
    }
}

fn run(steps: Vec<(String, BuildStep)>) {
    std::thread::sleep(START_DELAY);
    let started = Instant::now();
    let mut built = 0;
    for (app, step) in &steps {
        loop {
            if STOPPED.load(Ordering::SeqCst) {
                return;
            }
            if launch_is_building() {
                makepad_widgets::log!("prebuild: waiting while a launch builds");
                while launch_is_building() {
                    std::thread::sleep(POLL);
                }
            }
            let at = Instant::now();
            let mut cmd = clients::build_command(step);
            cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            lower_priority(&mut cmd);
            let mut child = match cmd.spawn() {
                Ok(child) => child,
                Err(error) => {
                    makepad_widgets::log!("prebuild: {app}: {error}; stopping");
                    return;
                }
            };
            RUNNING.store(child.id(), Ordering::SeqCst);
            let outcome = wait_or_step_aside(&mut child);
            RUNNING.store(0, Ordering::SeqCst);
            if STOPPED.load(Ordering::SeqCst) {
                return;
            }
            match outcome {
                Some(status) if status.success() => {
                    built += 1;
                    clients::note_built(step);
                    makepad_widgets::log!("prebuild: {app} built ({:.0} s)", at.elapsed().as_secs_f64());
                }
                Some(status) => {
                    makepad_widgets::log!("prebuild: {app}: cargo exited with {status}; its first open builds it");
                }
                None => {
                    makepad_widgets::log!("prebuild: {app}: stepped aside for a launch's build");
                    continue;
                }
            }
            break;
        }
    }
    makepad_widgets::log!("prebuild: {built} of {} built in {:.0} s", steps.len(), started.elapsed().as_secs_f64());
}

/// The build's exit status, or `None` when a launch started building first:
/// the build is stopped (its whole process group) and reaped.
fn wait_or_step_aside(child: &mut Child) -> Option<ExitStatus> {
    loop {
        if launch_is_building() || STOPPED.load(Ordering::SeqCst) {
            stop(child);
            return None;
        }
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => std::thread::sleep(POLL),
            Err(_) => {
                stop(child);
                return None;
            }
        }
    }
}

#[cfg(unix)]
fn stop(child: &mut Child) {
    // cargo and every rustc it started share the group `lower_priority` made.
    unsafe {
        libc::killpg(child.id() as libc::pid_t, libc::SIGTERM);
    }
    let _ = child.wait();
}

#[cfg(not(unix))]
fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Background priority for the build and everything it starts: on macOS
/// darwinbg (efficiency cores, throttled disk), nice 19 elsewhere on Unix,
/// below normal on Windows. Its own process group lets [`stop`] reach rustc.
#[cfg(unix)]
fn lower_priority(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
    // Only async-signal-safe calls between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            #[cfg(target_os = "macos")]
            libc::setpriority(libc::PRIO_DARWIN_PROCESS, 0, libc::PRIO_DARWIN_BG);
            libc::setpriority(libc::PRIO_PROCESS, 0, 19);
            Ok(())
        });
    }
}

#[cfg(windows)]
fn lower_priority(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
    cmd.creation_flags(BELOW_NORMAL_PRIORITY_CLASS);
}

#[cfg(not(any(unix, windows)))]
fn lower_priority(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_reads_like_the_warm_pools() {
        assert!(enabled(None));
        assert!(enabled(Some("")));
        assert!(enabled(Some("0")));
        assert!(!enabled(Some("1")));
        assert!(!enabled(Some("yes")));
    }

    fn files() -> AppDef {
        AppDef::app("files", "Files", "makepad-files", "apps/files", "files", clients::LaunchPolicy::OrFocus)
    }

    #[test]
    fn an_installed_shell_has_nothing_to_prebuild() {
        // No checkout: a launch runs the binary beside the shell, no build.
        assert!(plan_listed(&[files()], None, |_| true).is_empty());
    }

    #[test]
    fn a_checkout_prebuilds_the_launchs_own_build_once() {
        let root = Path::new("/checkout");
        let launch = clients::launch_plan(&files(), Some(root), &[]).unwrap().build.unwrap();
        let steps = plan_listed(&[files(), files()], Some(root), |_| true);
        assert_eq!(steps.len(), 1, "the same build twice runs once: {steps:?}");
        assert_eq!(steps[0].0, "files");
        assert_eq!(steps[0].1, launch, "exactly what the first open would build");
    }

    #[test]
    fn an_app_the_launcher_hides_is_not_prebuilt() {
        let steps = plan_listed(&[files()], Some(Path::new("/checkout")), |id| id != "files");
        assert!(steps.is_empty(), "{steps:?}");
    }
}
