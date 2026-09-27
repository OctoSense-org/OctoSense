// Windows reserves only 1 MiB for the main thread by default. OctoSense's
// startup evaluates and constructs a large Makepad script/widget tree on that
// thread, whose bounded peak exceeds the default before the first window is
// shown. /STACK changes the PE address-space reserve; pages are still committed
// only as they are used.
fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        println!("cargo:rustc-link-arg-bins=/STACK:16777216");
    }
}
