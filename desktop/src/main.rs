//! OctoSense on the desktop: the shell (crates/shell) and nothing else. The
//! window manager, hosting and phone layer are `octosense_shell`; this
//! package is the entry point and the desktop packaging (config/, upstream/,
//! scripts/, resources/android).
use octosense_shell::makepad_widgets::*;
use octosense_shell::App;

#[cfg(not(target_os = "linux"))]
octosense_shell::octosense_main!();

// GTK's native embedded child uses XEmbed. Prefer the session's existing
// XWayland display before Makepad selects a backend; explicit CLI overrides
// and Vulkan builds retain their own backend requirements.
#[cfg(target_os = "linux")]
mod linux_entry {
    use super::*;
    octosense_shell::octosense_main!();
    pub fn start() {
        main();
    }
}

/// The Linux windowing backend. The embedded WebKitGTK browser is an XEmbed
/// plug and needs the X11 backend, which on a Wayland session means the
/// whole shell renders through XWayland: on Hyprland that blocked the UI
/// thread in `eglSwapBuffers` for up to a second per idle repaint, stalled
/// alt-tab and menus 150–480 ms, and drew at the panel's unscaled size. So
/// X11 is opt-in: `OCTOSENSE_LINUX_BACKEND=x11` (or Makepad's own
/// `--linux-backend=x11`) for the embedded browser; otherwise Makepad picks
/// native Wayland when `WAYLAND_DISPLAY` is set, else X11.
#[cfg(target_os = "linux")]
pub fn app_main() {
    if std::env::var("OCTOSENSE_LINUX_BACKEND").map_or(false, |v| v.eq_ignore_ascii_case("x11")) {
        Cx::prefer_x11_for_embedded_browser();
    }
    linux_entry::start();
}

// This source is also the desktop library entry; keep both entry functions public.
#[cfg(target_os = "linux")]
pub fn main() {
    app_main();
}
