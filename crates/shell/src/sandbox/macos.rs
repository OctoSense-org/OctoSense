//! The macOS sandbox: a Seatbelt (SBPL) profile run through
//! `/usr/bin/sandbox-exec` (deprecated in Apple's headers, still the
//! mechanism; see the module above).
//!
//! The profile allows by default what a GPU app needs from the system
//! (libraries, fonts, the window server, Metal's shader cache) and closes
//! the person's data roots except the grants. In SBPL the last matching
//! rule wins, so the order below is the policy: close the roots, reopen the
//! ancestors' metadata (paths must resolve), the program read-only, the
//! jail, secrets and `external` grants; then close the host's private
//! directories again ([`Policy::private`]: the OctoSense home, the kernel's
//! core dir), whatever a grant opened, and reopen only the app's own jail
//! and secrets inside them.

use std::path::{Path, PathBuf};

use super::{ancestors, resolved, Access, Policy};
use crate::native_apps::Network;

/// The one binary the mechanism needs.
pub const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

fn quote(path: &Path) -> String {
    let s = path.to_string_lossy();
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn subpaths(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| format!(" (subpath {})", quote(p))).collect()
}

/// The profile text for `policy` (paths resolved, as the kernel sees them).
pub fn profile(policy: &Policy) -> String {
    let protected: Vec<PathBuf> = policy.protected.iter().map(|p| resolved(p)).collect();
    let program: Vec<PathBuf> = policy.program.iter().map(|p| resolved(p)).collect();
    let mut rw = vec![resolved(&policy.jail), resolved(&policy.secrets)];
    let mut ro = Vec::new();
    for (path, access) in &policy.external {
        match access {
            Access::ReadWrite => rw.push(resolved(path)),
            Access::Read => ro.push(resolved(path)),
        }
    }
    // Every ancestor of an opened path inside a closed root: metadata only.
    let mut metadata: Vec<PathBuf> = Vec::new();
    for path in rw.iter().chain(&ro).chain(&program) {
        for a in ancestors(path) {
            if protected.iter().any(|root| a.starts_with(root)) && !metadata.contains(&a) {
                metadata.push(a);
            }
        }
    }
    let mut out = String::new();
    out.push_str(&format!(";; OctoSense process sandbox for {} (ADR 0004 §3), generated\n", policy.app));
    out.push_str("(version 1)\n(allow default)\n");
    if !protected.is_empty() {
        out.push_str(&format!(";; the person's data roots are closed\n(deny file-read* file-write*{})\n", subpaths(&protected)));
    }
    if !metadata.is_empty() {
        let lits: String = metadata.iter().map(|p| format!(" (literal {})", quote(p))).collect();
        out.push_str(&format!("(allow file-read-metadata{lits})\n"));
    }
    if !program.is_empty() {
        out.push_str(&format!(";; its program and resources, read-only\n(allow file-read*{})\n", subpaths(&program)));
    }
    if !ro.is_empty() {
        out.push_str(&format!("(allow file-read*{})\n", subpaths(&ro)));
    }
    out.push_str(&format!(";; its jail, its secrets and external grants\n(allow file-read* file-write*{})\n", subpaths(&rw)));
    let private: Vec<PathBuf> = policy.private.iter().map(|p| resolved(p)).collect();
    if !private.is_empty() {
        let own = [resolved(&policy.jail), resolved(&policy.secrets)];
        out.push_str(&format!(
            ";; the host's private directories stay closed whatever a grant opened (peer tokens, other apps, the kernel)\n(deny file-read* file-write*{})\n",
            subpaths(&private)
        ));
        let mut inside: Vec<PathBuf> = Vec::new();
        for path in &own {
            for a in ancestors(path) {
                if private.iter().any(|root| a.starts_with(root)) && !inside.contains(&a) {
                    inside.push(a);
                }
            }
        }
        if !inside.is_empty() {
            let lits: String = inside.iter().map(|p| format!(" (literal {})", quote(p))).collect();
            out.push_str(&format!("(allow file-read-metadata{lits})\n"));
        }
        out.push_str(&format!(";; only its own jail and secrets inside them\n(allow file-read* file-write*{})\n", subpaths(&own)));
        // Its program may live inside them too (desktop builds go to
        // `<OctoSense home>/build`): without this, Metal cannot even read the
        // program's own bundle and the app crashes at start.
        let (reopened, kept_closed): (Vec<PathBuf>, Vec<PathBuf>) = program
            .iter()
            .filter(|p| private.iter().any(|root| p.starts_with(root)))
            .cloned()
            .partition(|p| super::program_reopenable(p, &private));
        for path in &kept_closed {
            makepad_widgets::log!("sandbox {}: program path {} holds private data; not reopened", policy.app, path.display());
            out.push_str(&format!(";; not reopened (it holds private data): {}\n", quote(path)));
        }
        if !reopened.is_empty() {
            out.push_str(&format!(";; its program, read-only, even inside them\n(allow file-read*{})\n", subpaths(&reopened)));
        }
    }
    if policy.network == Network::None {
        out.push_str(&format!(
            ";; network: the shell's hub only\n(deny network*)\n(allow network-outbound (remote ip \"localhost:{}\"))\n",
            policy.hub_port
        ));
    }
    if !policy.processes {
        out.push_str(";; no child processes\n(deny process-fork)\n");
        let execs: String = program.iter().map(|p| format!(" (subpath {})", quote(p))).collect();
        out.push_str(&format!("(deny process-exec)\n(allow process-exec{execs})\n"));
    }
    out
}

/// Write the profile where the child can be pointed at it (no whitespace:
/// cargo splits its `runner` on spaces).
pub fn write_profile(policy: &Policy) -> Result<PathBuf, String> {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = resolved(&std::env::temp_dir());
    let path = dir.join(format!("octosense-sandbox-{}-{}-{n}.sb", std::process::id(), policy.app));
    if path.to_string_lossy().chars().any(char::is_whitespace) {
        return Err(format!("the temp dir {} has whitespace", dir.display()));
    }
    std::fs::write(&path, profile(policy)).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

/// The cargo `runner` variable for this host's target triple.
pub fn runner_var() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER"
    } else {
        "CARGO_TARGET_X86_64_APPLE_DARWIN_RUNNER"
    }
}
