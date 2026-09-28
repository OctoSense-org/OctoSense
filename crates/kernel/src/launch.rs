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
//!   is the shell's [`crate::Options::program`] or `$OCTOS_APP_CORE_BIN`.
//!   Without one there is no kernel (a developer's own `octos serve` is never
//!   touched).
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
        let program = match inputs.program {
            Some(p) => p.to_path_buf(),
            None => std::env::var_os("OCTOS_APP_CORE_BIN")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| Unavailable::NoKernel("no kernel binary configured (OCTOS_APP_CORE_BIN)".into()))?,
        };
        if !program.is_file() {
            return Err(Unavailable::NoKernel(format!("{} is not a file", program.display())));
        }
        Ok(desktop_stdio(program, core_dir, inputs.env))
    }
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
pub(crate) fn prepare(launch: &Launch, core_dir: &Path) {
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
    // profile's tool policy, which octos reads at start.
    crate::system_tools::enforce(core_dir);
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
    #[test]
    fn desktop_without_a_program_has_no_kernel() {
        let missing = Path::new("/nonexistent/octos-kernel");
        let got = resolve(&Inputs { core_dir: Some(Path::new("/c")), program: Some(missing), env: &[] });
        assert!(matches!(got, Err(Unavailable::NoKernel(_))), "{got:?}");
        let got = resolve(&Inputs { core_dir: None, program: Some(missing), env: &[] });
        assert_eq!(got, Err(Unavailable::NoCoreDir));
    }
}
