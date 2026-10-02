//! How a kernel starts on this platform. Resolved afresh for every start, so
//! a changed configuration applies on the next start.
//!
//! - **Android**: `<nativeLibraryDir>/liboctos.so serve --stdio`, with
//!   `HOME=<kernel home>` and the kernel home as cwd. An app may exec only
//!   from its nativeLibraryDir, so the APK bundles the kernel as a "library"
//!   (`MAKEPAD_ANDROID_EXTRA_LIBS=liboctos.so=<octos>`). The environment and
//!   the kernel config merge are the ones AppCard's `stdio_spawn` used.
//! - **OpenHarmony**: the canonical core in-process
//!   (`octos_cli::embedded::serve_io`); HAP native libraries cannot exec.
//! - **Desktop**: `<program> serve --stdio --data-dir <core_dir> --config
//!   <core_dir>/config.json` with `OCTOS_HOME=<core_dir>`, where the program
//!   is the shell's [`crate::Options::program`], `$OCTOS_APP_CORE_BIN`, or
//!   the packaged `octos-kernel[.exe]` beside the shell executable (in a
//!   macOS `.app`, `Contents/MacOS` or `Contents/Resources`), in that order.
//!   The packaged kernel runs only when its receipt (`octos-kernel.json`,
//!   written by `tools/kernel-artifact.py --stage`) names the octos revision
//!   this build pins (checked when resolved, `verify_packaged`) and the
//!   kernel's SHA-256 (checked by [`prepare`] before each start, off the
//!   caller's thread). No PATH search and no attaching to another running
//!   kernel (a developer's own `octos serve` is never touched).
//! - **iOS**: no kernel (an app cannot exec a child).
//!
//! With Talk to Octos on (desktop and Android), `--stdio` becomes
//! `--host 127.0.0.1 --host-managed`: octos's host-owned loopback server
//! (see [`crate::network`]).

use std::path::{Path, PathBuf};

use crate::dirs;

/// A resolved way to start the kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// A loopback HTTP/WebSocket server shared by native and external clients.
    WebSocket {
        program: PathBuf,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: Option<PathBuf>,
    },
    /// `program args…` speaking NDJSON JSON-RPC on stdin/stdout.
    Stdio {
        program: PathBuf,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: Option<PathBuf>,
    },
    /// The canonical core served in-process from `home` (OpenHarmony).
    Embedded { home: PathBuf },
}

impl Launch {
    pub(crate) fn websocket(self) -> Self {
        match self {
            Self::Stdio { program, mut args, env, cwd } => {
                args.retain(|arg| arg != "--stdio");
                // octos's host-owned server: mandatory tokens, profiles in
                // this process, no solo login, stops on stdin EOF.
                args.extend(["--host".into(), "127.0.0.1".into(), "--host-managed".into()]);
                Self::WebSocket { program, args, env, cwd }
            }
            other => other,
        }
    }
}

/// Why no kernel can start here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// No core dir: no `Options::core_dir`, `$OCTOS_APP_CORE_DIR` or `$HOME`.
    NoCoreDir,
    /// No kernel binary: `why` names what was looked for.
    NoKernel(String),
    /// The platform cannot run one (iOS).
    Unsupported(&'static str),
}

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCoreDir => f.write_str("no octos core dir (set HOME or OCTOS_APP_CORE_DIR)"),
            Self::NoKernel(why) => write!(f, "no octos kernel: {why}"),
            Self::Unsupported(what) => write!(f, "no octos kernel on {what}"),
        }
    }
}

impl std::error::Error for Unavailable {}

/// What `resolve` needs: the core dir, the shell's program override and
/// extra environment.
pub(crate) struct Inputs<'a> {
    pub core_dir: Option<&'a Path>,
    pub program: Option<&'a Path>,
    pub env: &'a [(String, String)],
}

/// Resolve how to start the kernel on this platform. Checks, never creates:
/// [`prepare`] makes the directories right before a start.
pub(crate) fn resolve(inputs: &Inputs) -> Result<Launch, Unavailable> {
    let core_dir = inputs.core_dir.ok_or(Unavailable::NoCoreDir)?;
    #[cfg(target_env = "ohos")]
    {
        let _ = inputs.program;
        let _ = inputs.env;
        Ok(Launch::Embedded { home: dirs::kernel_home(core_dir) })
    }
    #[cfg(target_os = "ios")]
    {
        let _ = (core_dir, inputs.program, inputs.env);
        Err(Unavailable::Unsupported("iOS"))
    }
    #[cfg(target_os = "android")]
    {
        let program = match inputs.program {
            Some(p) => p.to_path_buf(),
            None => {
                let lib_dir = native_lib_dir()
                    .ok_or_else(|| Unavailable::NoKernel("the app's native lib dir was not found".into()))?;
                lib_dir.join("liboctos.so")
            }
        };
        if !program.is_file() {
            return Err(Unavailable::NoKernel(format!("{} is not bundled", program.display())));
        }
        Ok(phone_stdio(program, core_dir, inputs.env))
    }
    #[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
    {
        let env = std::env::var_os(PROGRAM_ENV).filter(|v| !v.is_empty()).map(PathBuf::from);
        // Through symlinks (a launcher linked into a bin dir): the kernel
        // travels with the real executable.
        let executable = std::env::current_exe().ok().map(|p| p.canonicalize().unwrap_or(p));
        let any_revision = std::env::var_os(ANY_REVISION_ENV).is_some_and(|v| v == "1");
        let program = desktop_program(inputs.program, env.as_deref(), executable.as_deref(), PINNED_REVISION, any_revision)?;
        Ok(desktop_stdio(program, core_dir, inputs.env))
    }
}

/// The desktop's explicit kernel: wins over the packaged one, unchecked (the
/// person chose it).
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
pub const PROGRAM_ENV: &str = "OCTOS_APP_CORE_BIN";
/// Development only: `=1` runs a packaged kernel whose receipt names another
/// octos revision than this build pins (a missing receipt or a SHA-256
/// mismatch is still refused).
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
pub const ANY_REVISION_ENV: &str = "OCTOSENSE_KERNEL_ANY_REVISION";
/// The packaged kernel's file name, beside the shell executable.
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
pub const PACKAGED_KERNEL: &str = if cfg!(windows) { "octos-kernel.exe" } else { "octos-kernel" };
/// Its receipt: `{"source", "revision", "version", "sha256"}`.
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
pub const PACKAGED_RECEIPT: &str = "octos-kernel.json";
/// The octos revision this build pins (the workspace's Cargo.lock, read by
/// build.rs); empty when the build could not tell.
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
pub const PINNED_REVISION: &str = env!("OCTOSENSE_PINNED_OCTOS_REVISION");

/// The kernel program on a desktop: `explicit` (`Options::program`), else
/// `env` (`$OCTOS_APP_CORE_BIN`), else the packaged kernel that travels with
/// `executable`, wherever it was installed or moved (never the working
/// directory). An override is authoritative: a broken one is an error, never
/// a fallback to another kernel. The packaged one must pass
/// [`verify_packaged`].
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
fn desktop_program(
    explicit: Option<&Path>,
    env: Option<&Path>,
    executable: Option<&Path>,
    pinned: &str,
    any_revision: bool,
) -> Result<PathBuf, Unavailable> {
    if let Some(program) = explicit.or(env) {
        check_executable(program)?;
        return Ok(program.to_owned());
    }
    let Some(dir) = executable.and_then(Path::parent) else {
        return Err(Unavailable::NoKernel(format!("no packaged kernel and no {PROGRAM_ENV} override")));
    };
    let Some(program) = packaged_candidates(dir).into_iter().find(|p| p.is_file()) else {
        return Err(Unavailable::NoKernel(format!(
            "no packaged {PACKAGED_KERNEL} beside {} and no {PROGRAM_ENV} override; \
             stage one with `python3 tools/kernel-artifact.py --host --stage <dir of the octosense binary>`",
            dir.display()
        )));
    };
    check_executable(&program)?;
    let receipt = receipt_candidates(dir, &program).into_iter().find(|p| p.is_file());
    let sha256 = verify_packaged(&program, receipt.as_deref(), pinned, any_revision)?;
    // Hashing a 100 MB kernel takes most of a second: not here (resolving
    // also answers status queries on the UI thread) but in `prepare`, right
    // before the start.
    expect_sha256(&program, sha256);
    Ok(program)
}

/// Where a packaged kernel may sit, relative to the shell executable's dir:
/// beside it (a source build's `target/<profile>`, the Windows install dir,
/// Linux `usr/bin`, a `.app`'s `Contents/MacOS`), or a `.app`'s
/// `Contents/Resources`.
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
fn packaged_candidates(dir: &Path) -> Vec<PathBuf> {
    let mut found = vec![dir.join(PACKAGED_KERNEL)];
    if let Some(resources) = bundle_resources(dir) {
        found.push(resources.join(PACKAGED_KERNEL));
    }
    found
}

/// Where its receipt may sit: beside the kernel, a `.app`'s
/// `Contents/Resources` (nothing but code belongs in `Contents/MacOS`), or a
/// Linux package's `usr/lib/octosense` (not `usr/bin`).
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
fn receipt_candidates(dir: &Path, program: &Path) -> Vec<PathBuf> {
    let mut found = vec![program.with_file_name(PACKAGED_RECEIPT)];
    if let Some(resources) = bundle_resources(dir) {
        found.push(resources.join(PACKAGED_RECEIPT));
    }
    if dir.file_name().is_some_and(|n| n == "bin") {
        if let Some(prefix) = dir.parent() {
            found.push(prefix.join("lib/octosense").join(PACKAGED_RECEIPT));
        }
    }
    found
}

/// `<bundle>.app/Contents/Resources` when `dir` is `<bundle>.app/Contents/MacOS`.
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
fn bundle_resources(dir: &Path) -> Option<PathBuf> {
    let contents = dir.parent()?;
    let is_bundle = dir.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && contents.parent()?.extension().is_some_and(|e| e == "app");
    is_bundle.then(|| contents.join("Resources"))
}

#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
fn check_executable(program: &Path) -> Result<(), Unavailable> {
    if !program.is_file() {
        return Err(Unavailable::NoKernel(format!("{} is not a file", program.display())));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(program).map(|m| m.permissions().mode() & 0o111 == 0).unwrap_or(true) {
            return Err(Unavailable::NoKernel(format!("{} is not executable", program.display())));
        }
    }
    Ok(())
}

/// A packaged kernel runs only when it is the one this build pins: its
/// receipt names `pinned` (unless `any_revision`, a development override)
/// and records a SHA-256, which [`prepare`] checks against the file before
/// every start ([`check_sha256`]). So an old `target/release/octos-kernel`
/// left from an earlier pin, a kernel without a receipt or a binary swapped
/// in later is refused with a reason the person sees, never run silently.
/// Returns the recorded SHA-256. Reads only the small receipt.
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
pub(crate) fn verify_packaged(program: &Path, receipt: Option<&Path>, pinned: &str, any_revision: bool) -> Result<String, Unavailable> {
    let refuse = |why: String| Err(Unavailable::NoKernel(refusal(program, &why)));
    let Some(receipt) = receipt else {
        return refuse(format!("it has no receipt ({PACKAGED_RECEIPT}) saying which octos revision it is"));
    };
    let text = match std::fs::read_to_string(receipt) {
        Ok(text) => text,
        Err(e) => return refuse(format!("its receipt {} cannot be read ({e})", receipt.display())),
    };
    let Ok(record) = serde_json::from_str::<serde_json::Value>(&text) else {
        return refuse(format!("its receipt {} is not JSON", receipt.display()));
    };
    let revision = record["revision"].as_str().unwrap_or("");
    let sha256 = record["sha256"].as_str().unwrap_or("").to_ascii_lowercase();
    if revision.is_empty() || sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return refuse(format!("its receipt {} names no revision and SHA-256", receipt.display()));
    }
    if revision != pinned {
        if pinned.is_empty() {
            if !any_revision {
                return refuse("this build does not know which octos revision it pins".into());
            }
        } else if any_revision {
            log::warn!(
                "octos-core: {ANY_REVISION_ENV}=1: running the packaged kernel at octos {} although this build pins {}",
                short(revision),
                short(pinned)
            );
        } else {
            return refuse(format!(
                "it is octos {} but this build pins {} (set {ANY_REVISION_ENV}=1 to run it anyway while developing)",
                short(revision),
                short(pinned)
            ));
        }
    }
    Ok(sha256)
}

#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
fn refusal(program: &Path, why: &str) -> String {
    format!(
        "refusing the packaged kernel {}: {why}. Rebuild it with `python3 tools/kernel-artifact.py --host --stage {}` \
         or name a kernel with {PROGRAM_ENV}",
        program.display(),
        program.parent().map(|p| p.display().to_string()).unwrap_or_default()
    )
}

/// Packaged kernels resolved, and the SHA-256 their receipts record.
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
type Expected = std::collections::HashMap<PathBuf, String>;
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
static EXPECTED: std::sync::Mutex<Option<Expected>> = std::sync::Mutex::new(None);

#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
fn expect_sha256(program: &Path, sha256: String) {
    EXPECTED.lock().unwrap().get_or_insert_with(Expected::new).insert(program.to_path_buf(), sha256);
}

/// Before a start (from [`prepare`], on the kernel's runtime): a packaged
/// kernel's bytes must be the ones its receipt records. A file already
/// checked (same path, size and modification time) is not hashed again.
/// Any other program (an override) has nothing to check.
#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
pub(crate) fn check_sha256(program: &Path) -> Result<(), String> {
    let Some(expected) = EXPECTED.lock().unwrap().as_ref().and_then(|e| e.get(program).cloned()) else {
        return Ok(());
    };
    // (path, recorded SHA-256) -> (size, modification time) when it matched.
    type Checked = Vec<((PathBuf, String), (u64, Option<std::time::SystemTime>))>;
    static CHECKED: std::sync::Mutex<Checked> = std::sync::Mutex::new(Vec::new());
    let stamp = std::fs::metadata(program).ok().map(|m| (m.len(), m.modified().ok()));
    let key = (program.to_path_buf(), expected);
    if let Some(stamp) = stamp {
        if CHECKED.lock().unwrap().iter().any(|(k, s)| *k == key && *s == stamp) {
            return Ok(());
        }
    }
    let actual = sha256_file(program).map_err(|e| refusal(program, &format!("it cannot be read ({e})")))?;
    if actual != key.1 {
        return Err(refusal(program, "its SHA-256 is not the one its receipt records"));
    }
    if let Some(stamp) = stamp {
        let mut checked = CHECKED.lock().unwrap();
        checked.retain(|(k, _)| k.0 != key.0);
        checked.push((key, stamp));
    }
    Ok(())
}

#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
fn short(revision: &str) -> &str {
    revision.get(..12).unwrap_or(revision)
}

#[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::Digest;
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The desktop launch: an explicit data dir, as AppCard's local core mode has
/// always run it, and `<core_dir>/config.json` when there is one (octos
/// refuses to start on a named config file that does not exist; without one
/// it uses its defaults).
#[cfg_attr(any(target_env = "ohos", target_os = "ios", target_os = "android"), allow(dead_code))]
pub(crate) fn desktop_stdio(program: PathBuf, core_dir: &Path, extra: &[(String, String)]) -> Launch {
    let data = core_dir.to_string_lossy().into_owned();
    let mut env = vec![
        ("OCTOS_HOME".to_owned(), data.clone()),
        ("OCTOS_OMIT_WORKSPACE_HINT".to_owned(), "1".to_owned()),
        ("RUST_LOG".to_owned(), "info".to_owned()),
    ];
    env.extend(extra.iter().cloned());
    let mut args = vec!["serve".into(), "--stdio".into(), "--data-dir".into(), data];
    let config = core_dir.join("config.json");
    if config.is_file() {
        args.push("--config".into());
        args.push(config.to_string_lossy().into_owned());
    }
    Launch::Stdio { program, args, env, cwd: Some(core_dir.join("workspace")) }
}

/// The Android launch, exactly as AppCard's `stdio_spawn` built it: HOME is
/// the kernel home (octos finds `<home>/.octos` from it), the a2app memory
/// tree is a skill read-zone, and the per-session workspace hint is left out
/// of prompts so the provider's prompt cache is reused across sessions.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn phone_stdio(program: PathBuf, core_dir: &Path, extra: &[(String, String)]) -> Launch {
    let home = dirs::kernel_home(core_dir);
    let mut args = vec!["serve".to_owned(), "--stdio".to_owned()];
    if !dirs::is_conventional(core_dir) {
        args.push("--data-dir".into());
        args.push(core_dir.to_string_lossy().into_owned());
    }
    let mut env = vec![
        ("HOME".to_owned(), home.to_string_lossy().into_owned()),
        // OCTOS_SKILLS_PATH adds the a2app memory dir as a skill READ-ZONE,
        // so a sub-agent's read_file reaches it by absolute path although
        // file tools are otherwise fenced to the per-session workspace.
        ("OCTOS_SKILLS_PATH".to_owned(), home.join("a2app").to_string_lossy().into_owned()),
        // The kernel's INFO trace reaches logcat through the stderr bridge.
        ("RUST_LOG".to_owned(), "info".to_owned()),
        // Byte-stable system prompts across sessions (server-side KV-cache
        // prefix reuse): the workspace hint is the only volatile byte.
        ("OCTOS_OMIT_WORKSPACE_HINT".to_owned(), "1".to_owned()),
    ];
    // Route the kernel's HTTPS through a proxy when the device has no route
    // of its own (an `adb reverse` tunnel): launch extra `makepad.OCTOS_PROXY`.
    if let Ok(proxy) = std::env::var("MAKEPAD_OCTOS_PROXY") {
        let proxy = proxy.trim().to_owned();
        if !proxy.is_empty() {
            for k in ["HTTPS_PROXY", "HTTP_PROXY", "https_proxy", "http_proxy", "ALL_PROXY"] {
                env.push((k.to_owned(), proxy.clone()));
            }
        }
    }
    env.extend(extra.iter().cloned());
    Launch::Stdio { program, args, env, cwd: Some(home) }
}

/// Make what a start needs: the core dir, the cwd (a missing cwd fails the
/// spawn's chdir with ENOENT, permanently, since the kernel would create it),
/// on Android the kernel config's memory budget (as AppCard did), and the
/// system agent's tool policy in the profile ([`crate::system_tools`]).
///
/// **Fails closed** (ADR 0004 §12, G13): when the tool policy cannot be
/// written and read back (a foreign policy in the profile, the person's own
/// octos home, an unwritable profile), `Err` says why and the kernel is not
/// started: every consumer's connection closes with that reason. A kernel
/// without the policy would give the system agent octos's shell. Likewise
/// on a desktop when a packaged kernel's bytes are not the ones its receipt
/// records ([`check_sha256`]).
pub(crate) fn prepare(launch: &Launch, core_dir: &Path) -> Result<(), String> {
    // A packaged kernel's bytes, against its receipt (never on the thread
    // that resolved it).
    #[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
    if let Launch::Stdio { program, .. } | Launch::WebSocket { program, .. } = launch {
        check_sha256(program)?;
    }
    if let Err(e) = std::fs::create_dir_all(core_dir) {
        log::warn!("octos-core: could not create {}: {e}", core_dir.display());
    }
    match launch {
        Launch::Stdio { cwd: Some(cwd), .. } | Launch::WebSocket { cwd: Some(cwd), .. } => {
            if let Err(e) = std::fs::create_dir_all(cwd) {
                log::warn!("octos-core: could not create {}: {e}", cwd.display());
            }
        }
        Launch::Stdio { .. } | Launch::WebSocket { .. } => {}
        Launch::Embedded { home } => {
            let _ = std::fs::create_dir_all(home);
        }
    }
    if cfg!(target_os = "android") {
        ensure_kernel_config(&dirs::kernel_home(core_dir));
    }
    // Every start: the system agent's tool set (ADR 0004 §12) as the
    // profile's tool policy, which octos reads at start. Refused: no start.
    if let crate::system_tools::Enforced::Refused(why) = crate::system_tools::enforce(core_dir) {
        return Err(format!(
            "The assistant was not started: OctoSense could not enforce its tool policy (no octos shell for any agent), because {why}"
        ));
    }
    // ... and the grants it starts with (a Settings change applies from the
    // next start: the shell offers a restart).
    crate::system_tools::take_grants_for_start();
    Ok(())
}

/// Floor for `memory.max_inject_tokens` in the phone kernel's config.
pub const INJECT_BUDGET_TOKENS: u64 = 40_000;

/// Ensure the phone kernel's config (`<home>/.config/octos/config.json`)
/// carries a `memory.max_inject_tokens` big enough for AppCard's a2app card
/// memory (octos's default, 2500, truncates the ~23k tree silently) and
/// `appui.sessions_in_cwd: false` (else the composer session's transcripts
/// land in the card tree). Merge-only: every other key is kept, a larger
/// explicit budget and an explicit `sessions_in_cwd` win, and an unparseable
/// file is left for the kernel to report. Moved from AppCard unchanged.
pub fn ensure_kernel_config(home: &Path) {
    let path = home.join(".config/octos/config.json");
    let mut root = match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(v) if v.is_object() => v,
            _ => {
                log::warn!("octos-core: {} is not a JSON object; memory budget NOT ensured", path.display());
                return;
            }
        },
        Err(_) => serde_json::json!({}),
    };
    let mut changed = false;
    {
        let memory = root.as_object_mut().unwrap().entry("memory").or_insert_with(|| serde_json::json!({}));
        match memory.as_object_mut() {
            Some(memory)
                if memory
                    .get("max_inject_tokens")
                    .and_then(|v| v.as_f64())
                    .map(|n| n < INJECT_BUDGET_TOKENS as f64)
                    .unwrap_or(!memory.contains_key("max_inject_tokens")) =>
            {
                memory.insert("max_inject_tokens".into(), serde_json::json!(INJECT_BUDGET_TOKENS));
                changed = true;
            }
            Some(_) => {}
            None => log::warn!("octos-core: kernel config `memory` is not an object; leaving it alone"),
        }
    }
    {
        let appui = root.as_object_mut().unwrap().entry("appui").or_insert_with(|| serde_json::json!({}));
        if let Some(appui) = appui.as_object_mut() {
            if !appui.contains_key("sessions_in_cwd") {
                appui.insert("sessions_in_cwd".into(), serde_json::json!(false));
                changed = true;
            }
        }
    }
    if !changed {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_vec_pretty(&root) {
        Ok(bytes) => match std::fs::write(&path, bytes) {
            Ok(()) => log::info!("octos-core: set memory.max_inject_tokens={INJECT_BUDGET_TOKENS} in {}", path.display()),
            Err(e) => log::warn!("octos-core: write {}: {e}", path.display()),
        },
        Err(e) => log::warn!("octos-core: serialize kernel config: {e}"),
    }
}

/// The directory holding the app's packaged native libraries, found from
/// our own mapped `libmakepad.so` in `/proc/self/maps` (the path carries a
/// per-install hash, and asking `ApplicationInfo` would need JNI).
#[cfg(any(target_os = "android", target_env = "ohos"))]
pub fn native_lib_dir() -> Option<PathBuf> {
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    for line in maps.lines() {
        let Some(slash) = line.find('/') else { continue };
        let path = &line[slash..];
        if path.ends_with("/libmakepad.so") {
            return Path::new(path).parent().map(Path::to_path_buf);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("octos-core-launch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn desktop_launch_names_the_core_dir_as_data_dir() {
        let extra = [("X".to_owned(), "1".to_owned())];
        let Launch::Stdio { args, env, cwd, .. } = desktop_stdio("/bin/octos".into(), Path::new("/c/core"), &extra) else {
            panic!("stdio")
        };
        assert_eq!(args, ["serve", "--stdio", "--data-dir", "/c/core"]);
        assert!(env.contains(&("OCTOS_HOME".into(), "/c/core".into())));
        assert_eq!(env.last(), Some(&("X".into(), "1".into())));
        assert_eq!(cwd, Some(PathBuf::from("/c/core/workspace")));
        // A config file in the core dir is passed along.
        let dir = tmp("desk");
        std::fs::write(dir.join("config.json"), "{}").unwrap();
        let Launch::Stdio { args, .. } = desktop_stdio("/bin/octos".into(), &dir, &[]) else { panic!("stdio") };
        assert_eq!(args[4..], ["--config".to_owned(), dir.join("config.json").to_string_lossy().into_owned()]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn phone_launch_is_home_based_for_a_dot_octos_core_dir() {
        let Launch::Stdio { args, env, cwd, .. } = phone_stdio("/lib/liboctos.so".into(), Path::new("/f/octos-home/.octos"), &[]) else {
            panic!("stdio")
        };
        assert_eq!(args, ["serve", "--stdio"]);
        assert!(env.contains(&("HOME".into(), "/f/octos-home".into())));
        assert!(env.contains(&("OCTOS_SKILLS_PATH".into(), "/f/octos-home/a2app".into())));
        assert_eq!(cwd, Some(PathBuf::from("/f/octos-home")));
        // Any other core dir is passed explicitly.
        let Launch::Stdio { args, .. } = phone_stdio("/lib/liboctos.so".into(), Path::new("/f/core"), &[]) else {
            panic!("stdio")
        };
        assert_eq!(args, ["serve", "--stdio", "--data-dir", "/f/core"]);
    }

    #[test]
    fn kernel_config_is_merged_not_replaced() {
        let home = tmp("cfg");
        let path = home.join(".config/octos/config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"keep":1,"memory":{"max_inject_tokens":2500.0}}"#).unwrap();
        ensure_kernel_config(&home);
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["keep"], 1);
        assert_eq!(v["memory"]["max_inject_tokens"], INJECT_BUDGET_TOKENS);
        assert_eq!(v["appui"]["sessions_in_cwd"], false);
        // An operator's larger budget and explicit knob win.
        std::fs::write(&path, r#"{"memory":{"max_inject_tokens":90000},"appui":{"sessions_in_cwd":true}}"#).unwrap();
        ensure_kernel_config(&home);
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["memory"]["max_inject_tokens"], 90000);
        assert_eq!(v["appui"]["sessions_in_cwd"], true);
        let _ = std::fs::remove_dir_all(home);
    }

    #[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
    const PIN: &str = "ae230ce04d57f3c29cf6c2518e5956a86c07d788";

    /// An executable fixture at `path` and, with `revision`, its receipt at
    /// `receipt` (default: beside it) recording that revision and its SHA-256.
    #[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
    fn fixture(path: &Path, receipt: Option<&Path>, revision: Option<&str>) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("fixture kernel {}", path.display())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        if let Some(revision) = revision {
            let receipt = receipt.map(Path::to_path_buf).unwrap_or_else(|| path.with_file_name(PACKAGED_RECEIPT));
            std::fs::create_dir_all(receipt.parent().unwrap()).unwrap();
            let sha = sha256_file(path).unwrap();
            std::fs::write(receipt, serde_json::json!({"revision": revision, "sha256": sha, "version": "octos test"}).to_string()).unwrap();
        }
    }

    #[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
    fn why(result: Result<PathBuf, Unavailable>) -> String {
        match result {
            Err(Unavailable::NoKernel(why)) => why,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
    #[test]
    fn desktop_finds_its_packaged_kernel_and_keeps_overrides_authoritative() {
        let dir = tmp("packaged");
        let shell = dir.join("octosense");
        let packaged = dir.join(PACKAGED_KERNEL);
        let explicit = dir.join("explicit");
        let env = dir.join("environment");
        fixture(&packaged, None, Some(PIN));
        fixture(&explicit, None, None);
        fixture(&env, None, None);
        assert_eq!(desktop_program(None, None, Some(&shell), PIN, false).unwrap(), packaged);
        assert_eq!(desktop_program(None, Some(&env), Some(&shell), PIN, false).unwrap(), env);
        assert_eq!(desktop_program(Some(&explicit), Some(&env), Some(&shell), PIN, false).unwrap(), explicit);
        // A broken override is an error, never a fallback to the packaged one.
        let missing = dir.join("missing");
        assert!(desktop_program(Some(&missing), Some(&env), Some(&shell), PIN, false).is_err());
        assert!(desktop_program(None, Some(&missing), Some(&shell), PIN, false).is_err());
        // Overrides are the person's choice: no receipt needed, any revision.
        assert_eq!(desktop_program(None, Some(&env), Some(&shell), "", false).unwrap(), env);
        // Found from the executable after the whole directory moves.
        let moved = dir.with_extension("moved");
        let _ = std::fs::remove_dir_all(&moved);
        std::fs::rename(&dir, &moved).unwrap();
        assert_eq!(desktop_program(None, None, Some(&moved.join("octosense")), PIN, false).unwrap(), moved.join(PACKAGED_KERNEL));
        std::fs::remove_file(moved.join(PACKAGED_KERNEL)).unwrap();
        assert!(why(desktop_program(None, None, Some(&moved.join("octosense")), PIN, false)).contains("kernel-artifact.py --host --stage"));
        assert!(desktop_program(None, None, None, PIN, false).is_err());
        std::fs::remove_dir_all(moved).unwrap();
    }

    #[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
    #[test]
    fn desktop_refuses_a_packaged_kernel_of_another_revision() {
        let dir = tmp("stale");
        let shell = dir.join("octosense");
        let packaged = dir.join(PACKAGED_KERNEL);
        // An old target/release/octos-kernel from an earlier pin.
        fixture(&packaged, None, Some(&"b".repeat(40)));
        let refused = why(desktop_program(None, None, Some(&shell), PIN, false));
        assert!(refused.contains("bbbbbbbbbbbb") && refused.contains(&PIN[..12]), "{refused}");
        assert!(refused.contains(ANY_REVISION_ENV), "{refused}");
        // The development override runs it anyway.
        assert_eq!(desktop_program(None, None, Some(&shell), PIN, true).unwrap(), packaged);
        // A build that does not know its pin refuses it too.
        assert!(why(desktop_program(None, None, Some(&shell), "", false)).contains("does not know"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
    #[test]
    fn desktop_refuses_a_packaged_kernel_without_a_matching_receipt() {
        let dir = tmp("receipt");
        let shell = dir.join("octosense");
        let packaged = dir.join(PACKAGED_KERNEL);
        fixture(&packaged, None, None);
        assert!(why(desktop_program(None, None, Some(&shell), PIN, true)).contains("no receipt"));
        std::fs::write(dir.join(PACKAGED_RECEIPT), "not json").unwrap();
        assert!(why(desktop_program(None, None, Some(&shell), PIN, false)).contains("not JSON"));
        std::fs::write(dir.join(PACKAGED_RECEIPT), r#"{"revision": "x", "sha256": "not a digest"}"#).unwrap();
        assert!(why(desktop_program(None, None, Some(&shell), PIN, true)).contains("no revision and SHA-256"));
        fixture(&packaged, None, Some(PIN));
        assert!(desktop_program(None, None, Some(&shell), PIN, false).is_ok());
        // The bytes are checked before the start (prepare), not when resolved.
        assert_eq!(check_sha256(&packaged), Ok(()));
        // The file changes after it was checked (another size): hashed again.
        std::fs::write(&packaged, "a different kernel binary").unwrap();
        assert!(desktop_program(None, None, Some(&shell), PIN, true).is_ok(), "resolving reads only the receipt");
        let refused = check_sha256(&packaged).unwrap_err();
        assert!(refused.contains("SHA-256"), "the development override never skips the hash: {refused}");
        let launch = desktop_stdio(packaged.clone(), &dir.join("core"), &[]);
        assert!(prepare(&launch, &dir.join("core")).unwrap_err().contains("SHA-256"), "no start");
        // An override is never hashed.
        assert_eq!(check_sha256(&dir.join("explicit")), Ok(()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
    #[test]
    fn desktop_finds_the_kernel_in_installed_layouts() {
        let dir = tmp("layouts");
        // macOS .app: the kernel in Contents/MacOS, its receipt in Resources.
        let macos = dir.join("OctoSense.app/Contents/MacOS");
        let resources = dir.join("OctoSense.app/Contents/Resources");
        fixture(&macos.join(PACKAGED_KERNEL), Some(&resources.join(PACKAGED_RECEIPT)), Some(PIN));
        assert_eq!(desktop_program(None, None, Some(&macos.join("octosense")), PIN, false).unwrap(), macos.join(PACKAGED_KERNEL));
        // ... or both in Resources.
        std::fs::remove_file(macos.join(PACKAGED_KERNEL)).unwrap();
        fixture(&resources.join(PACKAGED_KERNEL), None, Some(PIN));
        assert_eq!(desktop_program(None, None, Some(&macos.join("octosense")), PIN, false).unwrap(), resources.join(PACKAGED_KERNEL));
        // Not a bundle: a directory merely named MacOS has no Resources lookup.
        let plain = dir.join("plain/Contents/MacOS");
        std::fs::create_dir_all(&plain).unwrap();
        fixture(&dir.join("plain/Contents/Resources").join(PACKAGED_KERNEL), None, Some(PIN));
        assert!(desktop_program(None, None, Some(&plain.join("octosense")), PIN, false).is_err());
        // Linux package: usr/bin/octos-kernel, receipt in usr/lib/octosense.
        let bin = dir.join("usr/bin");
        fixture(&bin.join(PACKAGED_KERNEL), Some(&dir.join("usr/lib/octosense").join(PACKAGED_RECEIPT)), Some(PIN));
        assert_eq!(desktop_program(None, None, Some(&bin.join("octosense")), PIN, false).unwrap(), bin.join(PACKAGED_KERNEL));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(all(unix, not(any(target_env = "ohos", target_os = "ios", target_os = "android"))))]
    #[test]
    fn desktop_rejects_a_nonexecutable_kernel() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmp("not-executable");
        let program = dir.join(PACKAGED_KERNEL);
        fixture(&program, None, Some(PIN));
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(why(desktop_program(None, None, Some(&dir.join("octosense")), PIN, false)).contains("not executable"));
        assert!(desktop_program(Some(&program), None, None, PIN, false).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
    #[test]
    fn the_pinned_revision_is_the_workspace_lock() {
        let lock = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock")).unwrap();
        assert_eq!(PINNED_REVISION.len(), 40, "build.rs found the octos pin");
        assert!(lock.contains("name = \"octos-cli\"\nversion = "), "octos-cli is in the lock");
        assert!(lock.contains(&format!("octos.git?rev={PINNED_REVISION}#")), "{PINNED_REVISION} is the locked octos");
    }

    #[cfg(not(any(target_env = "ohos", target_os = "ios", target_os = "android")))]
    #[test]
    fn desktop_without_a_program_has_no_kernel() {
        let missing = Path::new("/nonexistent/octos-kernel");
        let got = resolve(&Inputs { core_dir: Some(Path::new("/c")), program: Some(missing), env: &[] });
        assert!(matches!(got, Err(Unavailable::NoKernel(_))), "{got:?}");
        let got = resolve(&Inputs { core_dir: None, program: Some(missing), env: &[] });
        assert_eq!(got, Err(Unavailable::NoCoreDir));
    }
}
