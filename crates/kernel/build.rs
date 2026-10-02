//! The octos revision this build pins, for the desktop launch check
//! (`launch::verify_packaged`): a packaged `octos-kernel` runs only when its
//! receipt names this revision.
//!
//! Read from the workspace's Cargo.lock (the `octos-cli` the Android and
//! desktop kernel artifacts are built from, as tools/kernel-artifact.py reads
//! it; else `octos-core`, which this crate links). `OCTOSENSE_OCTOS_REVISION`
//! at build time overrides it. Empty when neither is found: the desktop then
//! refuses a packaged kernel it cannot check (an explicit program still runs).

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-env-changed=OCTOSENSE_OCTOS_REVISION");
    let lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let revision = std::env::var("OCTOSENSE_OCTOS_REVISION")
        .ok()
        .filter(|r| is_revision(r))
        .or_else(|| std::fs::read_to_string(&lock).ok().and_then(|text| pinned(&text)))
        .unwrap_or_default();
    println!("cargo:rustc-env=OCTOSENSE_PINNED_OCTOS_REVISION={revision}");
}

fn is_revision(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// The one octos-org/octos revision of `octos-cli` (else `octos-core`) in a
/// Cargo.lock; none when absent or ambiguous.
fn pinned(lock: &str) -> Option<String> {
    const SOURCE: &str = "git+https://github.com/octos-org/octos.git?rev=";
    for crate_name in ["octos-cli", "octos-core"] {
        let mut found = Vec::new();
        for block in lock.split("[[package]]") {
            let field = |key: &str| {
                block.lines().find_map(|line| {
                    let rest = line.strip_prefix(key)?.trim_start().strip_prefix('=')?.trim();
                    Some(rest.trim_matches('"').to_owned())
                })
            };
            if field("name").as_deref() != Some(crate_name) {
                continue;
            }
            if let Some(rev) = field("source").and_then(|s| s.strip_prefix(SOURCE).map(|r| r.split('#').next().unwrap_or("").to_owned())) {
                if is_revision(&rev) && !found.contains(&rev) {
                    found.push(rev);
                }
            }
        }
        match found.len() {
            0 => continue,
            1 => return found.pop(),
            _ => return None,
        }
    }
    None
}
