//! The one place the standalone-shell condition is written — and the host's
//! build id.
//!
//! `mobile_only` is set for Android builds (cargo-makepad passes no features,
//! and a Cargo feature cannot be target-conditional) and for any build with
//! `--features mobile-only`. Code reads it as `#[cfg(mobile_only)]` /
//! `cfg!(mobile_only)`, never as the feature or the target directly.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(mobile_only)");
    let feature = std::env::var_os("CARGO_FEATURE_MOBILE_ONLY").is_some();
    let android = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android");
    if feature || android {
        println!("cargo:rustc-cfg=mobile_only");
    }
    // The host's build id: the second this build was configured, as digits.
    // A hosted AppCard pins its card approvals to the runtime it admitted
    // them under; when the host is a NEW build the store is archived once
    // for re-admission (`octosense_appcard::reapprove_cards_for_host_build`),
    // which is why any source change has to yield a new id.
    let build_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    println!("cargo:rustc-env=OCTOSENSE_BUILD_ID={build_id}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=apps");
    println!("cargo:rerun-if-changed=Cargo.lock");
}
