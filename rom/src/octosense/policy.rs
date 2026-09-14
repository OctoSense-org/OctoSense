/// Optional background work is enabled explicitly for this invocation.
pub fn requested(flag: &str) -> bool {
    std::env::args().any(|arg| arg == flag)
}

/// Select the shell for the build target, including when cross-compiling.
/// The standalone build has one shell, whatever it runs on.
pub fn startup_style() -> crate::desktop::DesktopStyle {
    startup_style_for_os(std::env::consts::OS, crate::MOBILE_ONLY)
}

fn startup_style_for_os(target_os: &str, mobile_only: bool) -> crate::desktop::DesktopStyle {
    use crate::desktop::DesktopStyle;
    if mobile_only { return DesktopStyle::Android; }
    match target_os {
        "android" => DesktopStyle::Android,
        "ios" => DesktopStyle::Ios,
        _ => DesktopStyle::Omarchy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::DesktopStyle;

    #[test]
    fn mobile_targets_start_with_their_touch_shell() {
        assert_eq!(startup_style_for_os("android", false), DesktopStyle::Android);
        assert_eq!(startup_style_for_os("ios", false), DesktopStyle::Ios);
    }

    #[test]
    fn the_standalone_shell_is_android_everywhere() {
        for target_os in ["android", "ios", "macos", "linux", "windows"] {
            assert_eq!(startup_style_for_os(target_os, true), DesktopStyle::Android);
        }
    }

    #[test]
    fn desktop_and_web_keep_the_existing_default() {
        for target_os in ["macos", "windows", "linux", "freebsd", "unknown"] {
            assert_eq!(startup_style_for_os(target_os, false), DesktopStyle::Omarchy);
        }
    }
}
