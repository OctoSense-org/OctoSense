//! OctoSense on the desktop: the shell (crates/shell) and nothing else. The
//! window manager, hosting and phone layer are `octosense_shell`; this
//! package is the entry point and the desktop packaging (config/, upstream/,
//! scripts/, resources/android).
use octosense_shell::makepad_widgets::*;
use octosense_shell::App;

#[cfg(not(all(target_os = "linux", not(target_env = "ohos"))))]
octosense_shell::octosense_main!();

// GTK's native embedded child uses XEmbed. Prefer the session's existing
// XWayland display before Makepad selects a backend; explicit CLI overrides
// and Vulkan builds retain their own backend requirements.
#[cfg(all(target_os = "linux", not(target_env = "ohos")))]
mod linux_entry {
    use super::*;
    octosense_shell::octosense_main!();
    pub fn start() {
        main();
    }
}

#[cfg(all(target_os = "linux", not(target_env = "ohos")))]
pub fn app_main() {
    Cx::prefer_x11_for_embedded_browser();
    linux_entry::start();
}

// This source is also the desktop library entry; keep both entry functions public.
#[cfg(all(target_os = "linux", not(target_env = "ohos")))]
pub fn main() {
    app_main();
}
