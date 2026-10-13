//! Out-of-process, same-user application replacement. Never elevates privileges.
fn main() {
    #[cfg(target_os = "macos")]
    if let Err(error) = octosense_updater::macos::helper_main() {
        eprintln!("OctoSense update: {error}");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("This helper supports macOS only.");
        std::process::exit(1);
    }
}
