//! `cfg(kernel)`: this build hosts the octos kernel — feature `octos-core`,
//! or a native mobile target, which always links it (a Cargo feature cannot
//! be target-conditional). The source reads the cfg, never the feature or
//! the target directly, so the two can never disagree.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(kernel)");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let native_mobile = target_os == "android" || target_os == "ios" || target_env == "ohos";
    if native_mobile || std::env::var_os("CARGO_FEATURE_OCTOS_CORE").is_some() {
        println!("cargo:rustc-cfg=kernel");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
