//! The one place the standalone-shell condition is written — and the host's
//! build id. (The Windows main-thread stack is a bin link arg: desktop/build.rs.)
//!
//! `mobile_only` is set for Android and OpenHarmony builds (cargo-makepad
//! passes no features, and a Cargo feature cannot be target-conditional) and
//! for any build with `--features mobile-only`. `native_mobile` is set for
//! Android, iOS and OpenHarmony. Code reads them as `#[cfg(mobile_only)]` /
//! `cfg!(native_mobile)`, never as the feature or the targets directly.
//!
//! `wasm_functions` marks a build that runs the `wasm` service (ADR 0011):
//! feature `wasm-functions` on macOS, Linux or Android, the targets
//! Cargo.toml links its runtime for. Code reads `#[cfg(wasm_functions)]`.
//!
//! `dev_mode` marks a development build (ADR 0004 §13, `src/dev_mode.rs`):
//! any build with debug assertions (`cargo build`, `cargo run`, tests), or
//! any build with `--features dev-mode`. A release build without the feature
//! honours developer mode only with the `--dev-grant-all` launch flag.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(dev_mode)");
    if std::env::var_os("CARGO_FEATURE_DEV_MODE").is_some() || std::env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_some() {
        println!("cargo:rustc-cfg=dev_mode");
    }
    println!("cargo:rerun-if-env-changed=OCTOSENSE_STORE_BUILD");
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
    println!("cargo:rustc-check-cfg=cfg(wasm_functions)");
    let wasm_target = target_os == "macos" || target_os == "android" || (target_os == "linux" && target_env != "ohos");
    if wasm_target && std::env::var_os("CARGO_FEATURE_WASM_FUNCTIONS").is_some() {
        println!("cargo:rustc-cfg=wasm_functions");
    }
    // `kernel`: this build hosts the octos kernel, exactly as
    // octosense-ai-host's build.rs decides it (feature `octos-core`, or a
    // native mobile target): `octosense_ai_host::kernel` exists. The system
    // chat (src/system_chat/) reads it.
    println!("cargo:rustc-check-cfg=cfg(kernel)");
    if native_mobile || std::env::var_os("CARGO_FEATURE_OCTOS_CORE").is_some() {
        println!("cargo:rustc-cfg=kernel");
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
