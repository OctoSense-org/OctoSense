//! macOS application replacement. Preparation runs on the updater worker;
//! the independent helper waits for an explicit commit AND process exit.
//! No shell commands or privilege escalation. App data stays in its profile.
//! Only code signed with the installed app's own identity is installed or
//! launched (`codesign.rs`); an unsigned source build uses the manual installer.
use crate::{Cancellation, Platform, VerifiedArtifact};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        },
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{mpsc, Once},
    time::{Duration, Instant, SystemTime},
};

mod codesign;
mod stage;
#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, String>;
const HELPER: &str = "octosense-update-helper";
const BUNDLE_ID: &str = "org.octosense.desktop";
const ACK_ENV: &str = "OCTOSENSE_UPDATE_JOB";
const ROLLED_BACK_ENV: &str = "OCTOSENSE_UPDATE_ROLLED_BACK";
const STARTUP_WAIT: Duration = Duration::from_secs(45);
const LEGACY_GRACE: Duration = Duration::from_secs(15);
/// How long the old host may take to exit after the commit.
const EXIT_WAIT: Duration = Duration::from_secs(180);
/// Bundled children outlive the host briefly: the kernel sidecar may drain
/// connections for 12 s after the shell closes its input (`crates/kernel`,
/// `SHARED_GRACE`). It is waited out, with backoff, before the bundle counts
/// as in use.
const STOP_WAIT: Duration = Duration::from_secs(20);
/// A retained update that cannot hold the previous app (never committed,
/// failed before the exchange, or rolled back) is deleted once this old.
const DISPOSABLE_AFTER: Duration = Duration::from_secs(10 * 60);
const MAX_FILES: usize = 50_000;
/// Debug builds only: set to `1`, unsigned fixture bundles may replace an
/// unsigned installation. Release builds contain neither this name nor code
/// that reads it (`Trust::of_installed`).
#[cfg(debug_assertions)]
const UNSIGNED_FIXTURES_ENV: &str = "OCTOSENSE_UPDATE_UNSIGNED_FIXTURES";

#[derive(Debug, Serialize, Deserialize)]
struct Ticket {
    protocol: u32,
    nonce: String,
    parent_pid: u32,
    target: PathBuf,
    old_digest: String,
    new_digest: String,
    new_tag: String,
    environment: BTreeMap<String, String>,
    startup_ack: bool,
    receipt: PathBuf,
}

/// Owns a helper waiting for the shell's normal, vetoable shutdown.
/// Dropping it before commit closes the pipe; the helper cancels and cleans up.
pub struct Prepared {
    child: Option<Child>,
    input: Option<ChildStdin>,
    nonce: String,
}
impl Prepared {
    /// Called once, only after every hosted app agreed to close.
    pub fn commit(mut self) -> Result<()> {
        let input = self.input.as_mut().ok_or("Update helper is unavailable.")?;
        writeln!(input, "commit {}", self.nonce)
            .map_err(|_| "Update helper stopped before installation.".to_string())?;
        input
            .flush()
            .map_err(|_| "Could not confirm the update.".to_string())?;
        self.input.take();
        // The helper must outlive this process; do not wait on the UI thread.
        self.child.take();
        Ok(())
    }
}
impl Drop for Prepared {
    fn drop(&mut self) {
        self.input.take();
        if let Some(mut child) = self.child.take() {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

/// Who may replace the installed app, and what the helper may launch.
enum Trust {
    /// Code satisfying the installed app's designated requirement and Team ID.
    Signed(codesign::Identity),
    /// Unsigned fixture bundles: unit tests pass it explicitly, and debug
    /// builds select it with `UNSIGNED_FIXTURES_ENV`. Not in release builds.
    #[cfg(any(test, debug_assertions))]
    Fixtures,
}
impl Trust {
    /// The installed app's identity. An unsigned or ad-hoc signed
    /// installation (a source or development build) is refused.
    fn of_installed(bundle: &Path) -> Result<Self> {
        #[cfg(debug_assertions)]
        if std::env::var_os(UNSIGNED_FIXTURES_ENV).is_some_and(|value| value == "1") {
            return Ok(Self::Fixtures);
        }
        codesign::Identity::installed(bundle).map(Self::Signed)
    }
    /// An error is a phrase that follows the bundle's name.
    fn admit(&self, bundle: &Path) -> Result<()> {
        match self {
            Self::Signed(identity) => identity.admit(bundle),
            #[cfg(any(test, debug_assertions))]
            Self::Fixtures => Ok(()),
        }
    }
}

/// Serializes installers for one installation folder with an exclusive lock
/// on the folder itself, so nothing is created beside the app. Released when
/// dropped or when the helper exits, however it exits; never inherited by
/// the apps it launches (std opens files close-on-exec).
struct InstallLock {
    _folder: File,
}
impl InstallLock {
    fn acquire(target: &Path) -> Result<Self> {
        let folder = target.parent().ok_or("No installation folder.")?;
        let folder = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(folder)
            .map_err(|e| format!("Could not lock the installation folder: {e}"))?;
        if unsafe { libc::flock(folder.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Another update is already installing.".into());
        }
        Ok(Self { _folder: folder })
    }
}

/// Whether this executable belongs to a packaged installation with
/// our helper. Permission errors during actual staging still fail closed.
pub fn available() -> bool {
    current_bundle().is_ok_and(|bundle| bundle.join("Contents/MacOS").join(HELPER).is_file())
}

fn current_bundle() -> Result<PathBuf> {
    let exe = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    bundle_for_executable(&exe)
}

fn bundle_for_executable(exe: &Path) -> Result<PathBuf> {
    let macos = exe.parent().ok_or("Missing executable folder.")?;
    let contents = macos.parent().ok_or("Missing bundle contents.")?;
    let bundle = contents.parent().ok_or("Missing app bundle.")?;
    if exe.file_name().is_none_or(|n| n != "octosense")
        || macos.file_name().is_none_or(|n| n != "MacOS")
        || contents.file_name().is_none_or(|n| n != "Contents")
        || bundle.extension().is_none_or(|n| n != "app")
    {
        return Err(
            "Install & Restart requires an installed OctoSense.app. Use Open installer instead."
                .into(),
        );
    }
    Ok(bundle.to_path_buf())
}

fn plist_field(bundle: &Path, field: &str) -> Result<String> {
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", field, "raw", "-o", "-"])
        .arg(bundle.join("Contents/Info.plist"))
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() || output.stdout.len() > 4096 {
        return Err("The app has invalid bundle metadata.".into());
    }
    Ok(String::from_utf8(output.stdout)
        .map_err(|e| e.to_string())?
        .trim()
        .to_owned())
}

fn validate_bundle(bundle: &Path, version: Option<&str>) -> Result<()> {
    if !fs::symlink_metadata(bundle).is_ok_and(|m| m.is_dir())
        || plist_field(bundle, "CFBundleIdentifier")? != BUNDLE_ID
        || plist_field(bundle, "CFBundleExecutable")? != "octosense"
        || version
            .is_some_and(|v| plist_field(bundle, "CFBundleShortVersionString").as_deref() != Ok(v))
    {
        return Err("The downloaded app does not match OctoSense's release identity.".into());
    }
    let exe = bundle.join("Contents/MacOS/octosense");
    if !fs::symlink_metadata(&exe).is_ok_and(|m| m.is_file() && m.mode() & 0o111 != 0) {
        return Err("The downloaded app has no executable.".into());
    }
    refuse_privileged_modes(bundle)
}

/// A setuid or setgid file would run as this user for anyone who opens it
/// from a shared folder such as /Applications. No OctoSense release has one.
fn refuse_privileged_modes(bundle: &Path) -> Result<()> {
    let mut pending = vec![bundle.to_path_buf()];
    let mut count = 0;
    while let Some(here) = pending.pop() {
        let meta = fs::symlink_metadata(&here).map_err(|e| e.to_string())?;
        if meta.mode() & 0o6000 != 0 {
            return Err("The app contains a setuid or setgid file.".into());
        }
        if !meta.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&here).map_err(|e| e.to_string())? {
            count += 1;
            if count > MAX_FILES {
                return Err("Too many files in the update.".into());
            }
            pending.push(entry.map_err(|e| e.to_string())?.path());
        }
    }
    Ok(())
}

fn private_dir(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err("The update staging folder is not private.".into());
    }
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    f.write_all(&serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| e.to_string())?;
    File::open(path.parent().ok_or("No update folder.")?)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}

fn read_ticket(job: &Path) -> Result<Ticket> {
    let root = job.parent().ok_or("Missing staging folder.")?;
    private_dir(root)?;
    if job.file_name().is_none_or(|n| n != "job.json")
        || root.canonicalize().map_err(|e| e.to_string())? != root
    {
        return Err("Invalid update job path.".into());
    }
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(job)
        .map_err(|e| e.to_string())?;
    let m = f.metadata().map_err(|e| e.to_string())?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
        || m.len() > 32768
    {
        return Err("Invalid update job file.".into());
    }
    let mut data = Vec::new();
    f.read_to_end(&mut data).map_err(|e| e.to_string())?;
    let ticket: Ticket = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if ticket.protocol != 1
        || ticket.parent_pid < 2
        || root.file_name().and_then(|n| n.to_str())
            != Some(&format!(".octosense-update-{}", ticket.nonce))
        || uuid::Uuid::parse_str(&ticket.nonce).is_err()
        || ticket.target.parent() != root.parent()
        || ticket.target.extension().is_none_or(|n| n != "app")
        || ticket.target == root
    {
        return Err("The update job has an invalid destination.".into());
    }
    for key in ticket.environment.keys() {
        if !environment_keys().contains(&key.as_str()) {
            return Err("Unexpected relaunch setting.".into());
        }
    }
    Ok(ticket)
}

fn environment_keys() -> Vec<&'static str> {
    let mut keys = vec![
        "HOME",
        "USER",
        "LOGNAME",
        "PATH",
        "LANG",
        "TMPDIR",
        "OCTOSENSE_HOME",
        "MAKEPAD_HOME",
        "OCTOS_APP_CORE_DIR",
        "OCTOSENSE_MAIL_VAULT",
        "OCTOSENSE_LLM_VAULT",
        "OCTOSENSE_SECRETS",
    ];
    if cfg!(debug_assertions) {
        keys.extend(["MAKEPAD_HIDE_WINDOWS", "MAKEPAD_REMOTE"]);
    }
    keys
}

/// Fingerprint of a bundle: names, contents, link targets and every
/// permission bit, setuid, setgid and sticky included.
fn digest_tree(root: &Path) -> Result<String> {
    fn walk(
        root: &Path,
        here: &Path,
        hash: &mut Sha256,
        count: &mut usize,
        bytes: &mut u64,
    ) -> Result<()> {
        let mut entries = fs::read_dir(here)
            .map_err(|e| e.to_string())?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            *count += 1;
            if *count > MAX_FILES {
                return Err("Too many files in the update.".into());
            }
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .as_os_str()
                .as_bytes();
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name);
            hash.update((meta.mode() & 0o7777).to_le_bytes());
            if meta.is_dir() {
                hash.update(b"directory");
                walk(root, &path, hash, count, bytes)?;
            } else if meta.file_type().is_symlink() {
                if !path
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .starts_with(root)
                {
                    return Err("An update resource points outside the app bundle.".into());
                }
                hash.update(b"link");
                let link = fs::read_link(&path).map_err(|e| e.to_string())?;
                let bytes = link.as_os_str().as_bytes();
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(bytes);
            } else if meta.is_file() {
                *bytes += meta.len();
                if *bytes > 4 * 1024 * 1024 * 1024 {
                    return Err("The app bundle is too large.".into());
                }
                hash.update(b"file");
                hash.update(meta.len().to_le_bytes());
                let mut f = File::open(&path).map_err(|e| e.to_string())?;
                let mut buffer = [0; 65536];
                loop {
                    let n = f.read(&mut buffer).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    hash.update(&buffer[..n]);
                }
            } else {
                return Err("Unsupported file in the app bundle.".into());
            }
        }
        Ok(())
    }
    if !fs::symlink_metadata(root).is_ok_and(|m| m.is_dir()) {
        return Err("The app bundle must be a directory, not a link.".into());
    }
    let mut hash = Sha256::new();
    walk(root, root, &mut hash, &mut 0, &mut 0)?;
    Ok(format!("{:x}", hash.finalize()))
}

/// Exchange whole bundles in one filesystem operation: even a power loss
/// cannot leave the installed path absent between two separate renames.
fn exchange(a: &Path, b: &Path) -> Result<()> {
    let a = CString::new(a.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let b = CString::new(b.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    if unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_SWAP) } != 0 {
        return Err(format!(
            "Could not replace the app: {}. Use Open installer instead.",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn process_exists(pid: u32) -> bool {
    unsafe {
        libc::kill(pid as i32, 0) == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

fn wait_for_exit(pid: u32, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    while process_exists(pid) {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

/// Refuse to replace a bundle still used by another instance or bundled child.
/// proc_pidpath reports executable paths without exposing command-line secrets.
fn ensure_bundle_stopped(bundle: &Path) -> Result<()> {
    let mut pids = vec![0i32; 65536];
    let bytes =
        unsafe { libc::proc_listpids(1, 0, pids.as_mut_ptr().cast(), (pids.len() * 4) as i32) };
    if bytes <= 0 || bytes as usize >= pids.len() * 4 {
        return Err("Could not check for running app instances. No replacement was made.".into());
    }
    for pid in pids
        .into_iter()
        .take(bytes as usize / 4)
        .filter(|pid| *pid > 0)
    {
        let mut path = [0u8; 4096];
        let size = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
        if size <= 0 {
            continue;
        } // Exited or a process owned by another user.
        let end = path.iter().position(|b| *b == 0).unwrap_or(path.len());
        let executable = Path::new(std::ffi::OsStr::from_bytes(&path[..end]));
        if executable.starts_with(bundle) {
            return Err("Another process is using this app bundle. Close its other instances before installing.".into());
        }
    }
    Ok(())
}

/// `ensure_bundle_stopped`, retried with backoff for up to `budget`, so the
/// old host's bundled children can finish their own shutdown first.
fn wait_until_stopped(bundle: &Path, budget: Duration) -> Result<()> {
    let deadline = Instant::now() + budget;
    let mut pause = Duration::from_millis(100);
    loop {
        match ensure_bundle_stopped(bundle) {
            Err(error) if Instant::now() + pause > deadline => return Err(error),
            Err(_) => {
                std::thread::sleep(pause);
                pause = (pause * 2).min(Duration::from_secs(2));
            }
            stopped => return stopped,
        }
    }
}

/// Idempotent recovery, including a helper interrupted on either side of the
/// atomic exchange. It never guesses from the journal alone, and it puts back
/// only code with the installed identity: a planted job cannot turn a
/// recovery into an installation.
fn restore_previous(
    ticket: &Ticket,
    job: &Path,
    candidate: &Path,
    trust: &Trust,
    message: &str,
) -> Result<()> {
    let installed = digest_tree(&ticket.target)?;
    let staged = digest_tree(candidate)?;
    if installed == ticket.new_digest && staged == ticket.old_digest {
        trust.admit(candidate).map_err(|reason| {
            format!("Recovery stopped: the retained app {reason}. Both bundles were preserved.")
        })?;
        exchange(&ticket.target, candidate)?;
    } else if installed != ticket.old_digest || staged != ticket.new_digest {
        return Err(
            "Recovery stopped because a bundle changed. Both bundles were preserved.".into(),
        );
    }
    phase(
        job.parent().ok_or("Missing staging folder.")?,
        "rolled_back",
    )?;
    status(ticket, "rolled_back", message)
}

fn launch(ticket: &Ticket, job: Option<&Path>) -> Result<Child> {
    let cache = ticket.receipt.parent().ok_or("No update log folder.")?;
    private_dir(cache)?;
    let log = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(cache.join(format!("relaunch-{}.log", ticket.nonce)))
        .map_err(|e| e.to_string())?;
    let mut command = Command::new(ticket.target.join("Contents/MacOS/octosense"));
    command
        .env_clear()
        .envs(&ticket.environment)
        .current_dir(ticket.target.parent().ok_or("No app folder.")?)
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);
    if let Some(job) = job {
        command.env(ACK_ENV, job);
    } else {
        command.env(ROLLED_BACK_ENV, "1");
    }
    command
        .spawn()
        .map_err(|e| format!("Could not relaunch OctoSense: {e}"))
}

/// The helper launches only what it has just checked: the bundle at the
/// installed path, with the installed identity. `job` starts the new app
/// (it acknowledges its first frame); `None` reopens the previous one with
/// the rollback signal.
fn launch_verified(ticket: &Ticket, job: Option<&Path>, trust: &Trust) -> Result<Child> {
    validate_bundle(&ticket.target, None)?;
    trust
        .admit(&ticket.target)
        .map_err(|reason| format!("The app to open {reason}, so it was not opened."))?;
    launch(ticket, job)
}

fn status(ticket: &Ticket, state: &str, message: &str) -> Result<()> {
    private_dir(ticket.receipt.parent().ok_or("No receipt folder.")?)?;
    write_json(
        &ticket.receipt,
        &serde_json::json!({"version":ticket.new_tag,"state":state,"message":message}),
    )
}

/// The staging folder's journal (`state.json`): what an interruption left,
/// and for the sweep, whether the folder may hold the previous app.
fn phase(root: &Path, phase: &str) -> Result<()> {
    write_json(
        &root.join("state.json"),
        &serde_json::json!({ "phase": phase }),
    )
}

/// Entrypoint of the separately packaged installer, copied outside the app
/// before launch. A pipe, not a command-line flag, commits the replacement.
pub fn helper_main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next().ok_or("Missing update job.")?;
    let restore = first == "--restore";
    let job = PathBuf::from(if restore {
        args.next().ok_or("Missing recovery job.")?
    } else {
        first
    });
    if args.next().is_some() {
        return Err("Unexpected helper arguments.".into());
    }
    let ticket = read_ticket(&job)?;
    let root = job.parent().unwrap();
    let candidate = root.join("candidate.app");
    let lock = InstallLock::acquire(&ticket.target)?;
    if restore {
        ensure_bundle_stopped(&ticket.target)?;
        let trust = Trust::of_installed(&ticket.target)?;
        restore_previous(
            &ticket,
            &job,
            &candidate,
            &trust,
            "The previous version was restored. Your app data was not replaced.",
        )?;
        println!("Previous app restored. You can open it normally; your profile was not replaced.");
        return Ok(());
    }
    println!("ready");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut instruction = String::new();
    std::io::stdin()
        .lock()
        .take(128)
        .read_line(&mut instruction)
        .map_err(|e| e.to_string())?;
    if instruction.trim_end() != format!("commit {}", ticket.nonce) {
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        return Ok(());
    }
    // Committed: the host is quitting. A journal write failing here must not
    // strand the person; the journal before the exchange is mandatory.
    let _ = phase(root, "waiting_for_exit");
    if !wait_for_exit(ticket.parent_pid, EXIT_WAIT) {
        return status(
            &ticket,
            "cancelled",
            "OctoSense did not close. The installed app was not changed.",
        );
    }
    let result = install_committed(&ticket, &job, &candidate, &Trust::of_installed, STOP_WAIT);
    if result.is_ok() {
        sweep_retained(&ticket.target, &lock, SystemTime::now());
    }
    result
}

/// Everything after the commit and the host's exit. The person must get an
/// app back from any failure: before the exchange the previous app is still
/// installed and is reopened; after it, the previous app is restored first.
/// Either way it opens with the rollback signal, and the receipt says why.
fn install_committed(
    ticket: &Ticket,
    job: &Path,
    candidate: &Path,
    trust_of: &dyn Fn(&Path) -> Result<Trust>,
    stop_wait: Duration,
) -> Result<()> {
    let root = job.parent().ok_or("Missing staging folder.")?;
    // The installed app's identity, taken before anything changes: it decides
    // what may replace it, and what may be opened if this fails.
    let trust = match trust_of(&ticket.target) {
        Ok(trust) => trust,
        Err(error) => {
            // Nothing verified to reopen. The next start shows the receipt.
            let message = format!("The update was not installed. {error}");
            let _ = phase(root, "not_installed");
            let _ = status(ticket, "failed", &message);
            return Err(message);
        }
    };
    if let Err(error) = exchange_verified(ticket, root, candidate, &trust, stop_wait) {
        let message =
            format!("The update was not installed. {error} The installed app was not changed.");
        let _ = phase(root, "not_installed");
        let _ = status(ticket, "failed", &message);
        launch_verified(ticket, None, &trust).map_err(|e| format!("{message} {e}"))?;
        return Err(message);
    }
    finish_exchanged(ticket, job, candidate, &trust, stop_wait)
}

/// After the exchange, the former app lives at candidate.app. Start the new
/// one, checked again first, or put the former one back.
fn finish_exchanged(
    ticket: &Ticket,
    job: &Path,
    candidate: &Path,
    trust: &Trust,
    stop_wait: Duration,
) -> Result<()> {
    let root = job.parent().ok_or("Missing staging folder.")?;
    if let Err(error) = start_new(ticket, job, root, trust) {
        return roll_back(ticket, job, candidate, trust, &error, stop_wait);
    }
    write_json(
        &root.join("state.json"),
        &serde_json::json!({"phase":"complete","startup_ack":ticket.startup_ack}),
    )?;
    status(
        ticket,
        "complete",
        if ticket.startup_ack {
            "Update installed and startup confirmed. The previous app is retained for recovery."
        } else {
            "Update installed. The older release stayed running for 15 seconds; it has no startup acknowledgement. The previous app is retained for recovery."
        },
    )
}

/// Every check before the one atomic exchange; nothing changes until all pass.
fn exchange_verified(
    ticket: &Ticket,
    root: &Path,
    candidate: &Path,
    trust: &Trust,
    stop_wait: Duration,
) -> Result<()> {
    wait_until_stopped(&ticket.target, stop_wait)?;
    if digest_tree(&ticket.target)? != ticket.old_digest
        || digest_tree(candidate)? != ticket.new_digest
    {
        return Err("The installed app or staged update changed after it was prepared.".into());
    }
    validate_bundle(candidate, ticket.new_tag.strip_prefix("desktop-v"))?;
    trust
        .admit(candidate)
        .map_err(|reason| format!("The update {reason}."))?;
    phase(root, "swapping")?;
    exchange(&ticket.target, candidate)
}

/// Starts the exchanged app, checked again first, and waits for its first
/// frame (or, for an older release, for it to stay alive).
fn start_new(ticket: &Ticket, job: &Path, root: &Path, trust: &Trust) -> Result<()> {
    phase(root, "starting")?;
    let mut child = launch_verified(ticket, Some(job), trust)?;
    let deadline = Instant::now()
        + if ticket.startup_ack {
            STARTUP_WAIT
        } else {
            LEGACY_GRACE
        };
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("The new app exited before startup completed.".to_string());
        }
        if ticket.startup_ack
            && fs::read_to_string(root.join("started")).is_ok_and(|s| s == ticket.nonce)
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            if !ticket.startup_ack {
                return Ok(());
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err("The new app did not confirm startup.".to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The new app did not start, or failed its check before launch: put the
/// previous app back and reopen it. Only the previous app must be exactly as
/// it was; whatever is installed goes back into the staging folder even if it
/// changed after its checks, so unverified code never stays installed. Nothing
/// is opened only when the previous app changed or the bundle stays in use;
/// the receipt then explains, and both bundles are kept.
fn roll_back(
    ticket: &Ticket,
    job: &Path,
    candidate: &Path,
    trust: &Trust,
    error: &str,
    stop_wait: Duration,
) -> Result<()> {
    let root = job.parent().ok_or("Missing staging folder.")?;
    let restored = wait_until_stopped(&ticket.target, stop_wait).and_then(|()| {
        if digest_tree(candidate)? != ticket.old_digest {
            return Err(
                "Automatic recovery stopped because the retained previous app changed.".into(),
            );
        }
        trust.admit(candidate).map_err(|reason| {
            format!("Automatic recovery stopped: the retained previous app {reason}.")
        })?;
        exchange(&ticket.target, candidate)
    });
    if let Err(stopped) = restored {
        let message = format!(
            "{error} {stopped} Both bundles are kept in {}; with OctoSense closed, the helper there can restore the previous app with --restore.",
            root.display()
        );
        let _ = phase(root, "recovery_stopped");
        let _ = status(ticket, "failed", &message);
        return Err(message);
    }
    let _ = phase(root, "rolled_back");
    let _ = status(
        ticket,
        "rolled_back",
        &format!("{error} The previous version was restored. Your app data was not replaced."),
    );
    launch_verified(ticket, None, trust).map(drop)
}

/// Called after the shell draws its first frame. Receipt IO runs off the UI
/// thread; only the expected executable at the exact installed path can ACK.
pub fn acknowledge_startup() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Some(job) = std::env::var_os(ACK_ENV).map(PathBuf::from) else {
            return;
        };
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                let ticket = read_ticket(&job)?;
                if current_bundle()? != ticket.target
                    || digest_tree(&ticket.target)? != ticket.new_digest
                {
                    return Err("Relaunch does not match the prepared update.".into());
                }
                let path = job.parent().unwrap().join("started");
                let mut f = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(path)
                    .map_err(|e| e.to_string())?;
                f.write_all(ticket.nonce.as_bytes())
                    .map_err(|e| e.to_string())?;
                f.sync_all().map_err(|e| e.to_string())
            })();
            if let Err(error) = result {
                eprintln!("Update startup acknowledgement: {error}");
            }
        });
    });
}

/// Read once at startup, off the UI thread: why the last update attempt did
/// not finish, as (title, message), so a failure reaches the person without
/// a log. The receipt is then marked seen. A freshly installed app that is
/// still being verified reports nothing: its helper writes the outcome later.
pub fn take_install_notice(cache: &Path) -> Option<(String, String)> {
    if std::env::var_os(ACK_ENV).is_some() {
        return None;
    }
    let path = cache.join("last-install.json");
    let unseen = read_receipt(&path)
        .filter(|record| record["seen"] != true)
        .and_then(|mut record| {
            let (title, fallback) = match record["state"].as_str()? {
                "failed" => ("Update not installed", "The update could not be installed."),
                "cancelled" => ("Update cancelled", "The update was cancelled."),
                "rolled_back" => ("Update rolled back", REOPENED),
                _ => return None,
            };
            let message: String = record["message"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_control())
                .take(600)
                .collect();
            record["seen"] = true.into();
            let _ = write_json(&path, &record);
            Some((
                title.to_string(),
                if message.is_empty() {
                    fallback.to_string()
                } else {
                    message
                },
            ))
        });
    unseen.or_else(|| {
        (std::env::var(ROLLED_BACK_ENV).as_deref() == Ok("1"))
            .then(|| ("Update rolled back".to_string(), REOPENED.to_string()))
    })
}
const REOPENED: &str =
    "The new version could not start. Your previous app was reopened and your profile was kept.";

fn read_receipt(path: &Path) -> Option<serde_json::Value> {
    private_dir(path.parent()?).ok()?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > 16384 {
        return None;
    }
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;
    serde_json::from_slice(&data).ok()
}

/// Deletes retained updates beside this installation that nothing needs;
/// each holds a whole app. Run at startup, off the UI thread, and by the
/// helper after a successful installation. Returns how many were removed.
pub fn sweep_backups() -> usize {
    let Ok(target) = current_bundle() else {
        return 0;
    };
    // An installer at work holds the lock: leave everything to it.
    let Ok(lock) = InstallLock::acquire(&target) else {
        return 0;
    };
    sweep_retained(&target, &lock, SystemTime::now())
}

/// Keeps the newest retained update that may hold the previous app (the one
/// `--restore` needs) and removes older ones; an attempt that never replaced
/// anything goes once it is `DISPOSABLE_AFTER` old. Only private
/// `.octosense-update-<uuid>` folders with a valid job for this exact
/// installation are considered.
fn sweep_retained(target: &Path, _lock: &InstallLock, now: SystemTime) -> usize {
    let Some(folder) = target.parent() else {
        return 0;
    };
    let Ok(entries) = fs::read_dir(folder) else {
        return 0;
    };
    let mut retained = Vec::new();
    for entry in entries.flatten().take(4096) {
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_prefix(".octosense-update-"))
        else {
            continue;
        };
        let root = entry.path();
        let job = root.join("job.json");
        // read_ticket requires a private folder owned by this user, named
        // for its own job. A folder still being prepared has no job yet.
        if uuid::Uuid::parse_str(id).is_err()
            || read_ticket(&job).map(|ticket| ticket.target) != Ok(target.to_path_buf())
        {
            continue;
        }
        let Ok(written) = fs::symlink_metadata(&job).and_then(|m| m.modified()) else {
            continue;
        };
        retained.push((written, may_hold_previous_app(&root), root));
    }
    retained.sort_by_key(|(written, _, _)| std::cmp::Reverse(*written));
    let keep = retained.iter().position(|(_, holds, _)| *holds);
    let mut removed = 0;
    for (index, (written, holds, root)) in retained.iter().enumerate() {
        let settled = now
            .duration_since(*written)
            .is_ok_and(|age| age >= DISPOSABLE_AFTER);
        if Some(index) != keep && (*holds || settled) && remove_retained(root) {
            removed += 1;
        }
    }
    removed
}

/// Whether a retained update may hold the previous app: from the exchange on,
/// or with a journal that cannot be read. Never committed, cancelled, failed
/// before the exchange, or rolled back: it holds only an unused update.
fn may_hold_previous_app(root: &Path) -> bool {
    match fs::read(root.join("state.json")) {
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
        Ok(data) => !matches!(
            serde_json::from_slice::<serde_json::Value>(&data)
                .ok()
                .as_ref()
                .and_then(|state| state["phase"].as_str()),
            Some("waiting_for_exit" | "not_installed" | "rolled_back")
        ),
    }
}

/// Removes one retained update, bounded: links are removed, never followed,
/// and nothing is removed if any entry lies on another device (a disk image
/// still attached in it) or the tree is larger than an update can be.
fn remove_retained(root: &Path) -> bool {
    let Ok(device) = fs::symlink_metadata(root).map(|m| m.dev()) else {
        return false;
    };
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0;
    while let Some(here) = pending.pop() {
        let Ok(entries) = fs::read_dir(&here) else {
            return false;
        };
        for entry in entries {
            count += 1;
            let Ok((path, meta)) = entry.and_then(|entry| {
                let path = entry.path();
                fs::symlink_metadata(&path).map(|meta| (path, meta))
            }) else {
                return false;
            };
            if count > 2 * MAX_FILES || meta.dev() != device {
                return false;
            }
            if meta.is_dir() {
                pending.push(path);
            }
        }
    }
    fs::remove_dir_all(root).is_ok()
}

pub use stage::prepare;
