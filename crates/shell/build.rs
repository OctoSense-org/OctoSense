//! The one place the standalone-shell condition is written — and the host's
//! build id. (The Windows main-thread stack is a bin link arg: desktop/build.rs.)
//!
//! `mobile_only` is set for Android and OpenHarmony builds (cargo-makepad
//! passes no features, and a Cargo feature cannot be target-conditional) and
//! for any build with `--features mobile-only`. `native_mobile` is set for
//! Android, iOS and OpenHarmony. Code reads them as `#[cfg(mobile_only)]` /
//! `cfg!(native_mobile)`, never as the feature or the targets directly.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(mobile_only)");
    let feature = std::env::var_os("CARGO_FEATURE_MOBILE_ONLY").is_some();
    println!("cargo:rustc-check-cfg=cfg(native_mobile)");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    // OpenHarmony is Linux with target_env=ohos; it cannot host desktop processes.
    let native_mobile = target_os == "android" || target_os == "ios" || target_env == "ohos";
    if native_mobile {
        println!("cargo:rustc-cfg=native_mobile");
    }
    if feature || target_os == "android" || target_env == "ohos" {
        println!("cargo:rustc-cfg=mobile_only");
    }
    // `makepad_vulkan`: Makepad renders with Vulkan (`MAKEPAD=vulkan`, the
    // same switch makepad-platform's build reads), so a Linux desktop can
    // share a process app's frames zero-copy (native-apps.json's
    // `process-if-vulkan`, apps::vulkan_wayland).
    println!("cargo:rustc-check-cfg=cfg(makepad_vulkan)");
    println!("cargo:rerun-if-env-changed=MAKEPAD");
    let makepad = std::env::var("MAKEPAD").unwrap_or_default();
    if makepad.split(['+', ',']).any(|config| matches!(config, "vulkan" | "use_vulkan" | "quest")) {
        println!("cargo:rustc-cfg=makepad_vulkan");
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
    println!("cargo:rerun-if-changed=../../Cargo.lock");
}
