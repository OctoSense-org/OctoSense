//! The one place the standalone-shell condition is written.
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
    println!("cargo:rerun-if-changed=build.rs");
}
