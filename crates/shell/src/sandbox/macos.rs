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
//! and secrets inside them; then make everything the next build reads or
//! runs read-only again ([`Policy::read_only`]: the checkout, target dir,
//! cargo and rustup homes, `.cargo/` and toolchain files up the tree), and
//! last the fixed hardening ([`HARDENING`]): login items
//! (`~/Library/LaunchAgents`) not writable, whatever a home grant says,
//! since launchd starts them outside any sandbox. (The keychains stay as
//! the system guards them: closing their files broke `git`'s
//! `osxkeychain` credentials in the Terminal, tried 2026-09-30, and the
//! files are encrypted; the Keychain's own access prompts are the control.)
//!
//! Generated profiles live in `<OctoSense home>/sandbox/` (owner-only, and
//! closed to every app like the rest of that home), not the shared temp dir
//! where another process could swap one before `sandbox-exec` reads it.

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
    if !policy.read_only.is_empty() {
        let read_only: Vec<PathBuf> = policy.read_only.iter().map(|p| resolved(p)).collect();
        out.push_str(&format!(
            ";; what the next build reads or runs stays read-only, whatever a grant opened\n(deny file-write*{})\n",
            subpaths(&read_only)
        ));
        let own: Vec<PathBuf> = [resolved(&policy.jail), resolved(&policy.secrets)]
            .into_iter()
            .filter(|p| read_only.iter().any(|ro| p.starts_with(ro)))
            .collect();
        if !own.is_empty() {
            out.push_str(&format!(";; its own jail and secrets even there\n(allow file-write*{})\n", subpaths(&own)));
        }
    }
    if let Some(home) = policy.protected.first() {
        out.push_str(&hardening(&resolved(home)));
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

/// Under the person's home: never writable, whatever a grant opened (login
/// items, which launchd starts outside any sandbox at the next login).
pub const HARDENING: &[&str] = &["Library/LaunchAgents"];

/// The fixed hardening for the person's home `home` ([`HARDENING`]).
fn hardening(home: &Path) -> String {
    let no_write: Vec<PathBuf> = HARDENING.iter().map(|rel| home.join(rel)).collect();
    format!(";; login items, whatever a home grant says\n(deny file-write*{})\n", subpaths(&no_write))
}

/// Write the profile into `<OctoSense home>/sandbox/`, where only the
/// shell reaches it.
pub fn write_profile(policy: &Policy) -> Result<PathBuf, String> {
    let dir = crate::octosense::paths::private_dir("sandbox").map_err(|e| format!("the profile folder: {e}"))?;
    write_profile_in(&dir, policy)
}

fn write_profile_in(dir: &Path, policy: &Policy) -> Result<PathBuf, String> {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = resolved(dir).join(format!("octosense-sandbox-{}-{}-{n}.sb", std::process::id(), policy.app));
    std::fs::write(&path, profile(policy)).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}
