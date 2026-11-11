//! What a process app may ask of its own window (ADR 0015, "An app's own window"):
//! a size for its content ([`WmRequest::Resize`]), one window per set of launch
//! arguments ([`WmRequest::LaunchOrFocus`]), and a frame kept from one run to the
//! next ([`WmRequest::FrameAutosave`]), as a Mac's `setContentSize`,
//! single-instance windows and `setFrameAutosaveName` do.
//!
//! The frames live in the shell's own state, `<OctoSense home>/window-frames`, a
//! line a frame: `<app id>/<name> x y w h` in desk points. Apps never read or
//! write the file; a frame is keyed by the asking client's own app id, so an app
//! can only name frames of its own.
//!
//! [`WmRequest::Resize`]: makepad_wm_api::WmRequest::Resize
//! [`WmRequest::LaunchOrFocus`]: makepad_wm_api::WmRequest::LaunchOrFocus
//! [`WmRequest::FrameAutosave`]: makepad_wm_api::WmRequest::FrameAutosave

use std::path::{Path, PathBuf};

use crate::clients::ClientSlot;
use crate::desktop_layout::{self, DesktopWindow};
use crate::layout::{ClientId, LRect};

/// How many frames the file keeps; the ones saved longest ago go first.
pub const MAX_FRAMES: usize = 64;

/// Whether an app's own resize applies to its window: on a desktop whose windows
/// float, to a window that floats, not one maximized, full screen or snapped (a
/// window that fills something keeps its place until the person leaves that).
pub fn may_resize(mobile: bool, desktop_enabled: bool, window: Option<&DesktopWindow>) -> bool {
    !mobile && desktop_enabled && window.is_some_and(|w| !w.maximized && !w.fullscreen && w.snap.is_none() && !w.minimized)
}

/// The window an app asks to have `width` x `height` points of content under a
/// `title_h` title bar: its top left kept, kept to the room above the dock and
/// to the smallest window ([`desktop_layout::resized`]). `None` for a size that
/// is not one (zero, negative or not finite).
pub fn resize_rect(rect: LRect, width: f64, height: f64, title_h: f64, area: LRect, dock: f64) -> Option<LRect> {
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return None;
    }
    let (w, h) = (width, height + title_h.max(0.0));
    Some(desktop_layout::resized(rect, w - rect.w, h - rect.h, area, dock))
}

/// The open window of `app` started with exactly `args`, the oldest if several:
/// what [`WmRequest::LaunchOrFocus`](makepad_wm_api::WmRequest::LaunchOrFocus)
/// brings to the front. A warm standby, the assistant's pane, a Quick Look
/// preview, a window on its way out and a client with no window are not windows.
pub fn window_for<'a>(
    app: &str,
    args: &[String],
    clients: impl IntoIterator<Item = (&'a ClientId, &'a ClientSlot)>,
    has_window: impl Fn(ClientId) -> bool,
) -> Option<ClientId> {
    clients
        .into_iter()
        .filter(|(id, slot)| {
            slot.app == app
                && slot.launch_args == args
                && !slot.warm
                && !slot.pane
                && !slot.is_preview
                && !slot.stopped
                && slot.closing.is_none()
                && has_window(**id)
        })
        .map(|(id, _)| *id)
        .min()
}

/// The key a frame is kept under: the asking app's own id and the name it gave,
/// `None` for a name [`makepad_wm_api::valid_frame_name`] refuses.
pub fn key(app: &str, name: &str) -> Option<String> {
    let app_ok = !app.is_empty() && !app.chars().any(|c| c.is_whitespace() || c.is_control());
    (app_ok && makepad_wm_api::valid_frame_name(name)).then(|| format!("{app}/{name}"))
}

/// The frames kept, oldest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frames {
    entries: Vec<(String, LRect)>,
}

impl Frames {
    /// The file's lines; a line that is not a frame is skipped.
    pub fn parse(text: &str) -> Frames {
        let mut frames = Frames::default();
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let (Some(key), Some(x), Some(y), Some(w), Some(h), None) =
                (parts.next(), parts.next(), parts.next(), parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let numbers: Option<Vec<f64>> = [x, y, w, h].iter().map(|v| v.parse::<f64>().ok().filter(|v| v.is_finite())).collect();
            let Some(n) = numbers else { continue };
            if n[2] <= 0.0 || n[3] <= 0.0 || !key.contains('/') {
                continue;
            }
            frames.put(key, LRect::new(n[0], n[1], n[2], n[3]));
        }
        frames
    }

    pub fn to_text(&self) -> String {
        self.entries.iter().map(|(key, r)| format!("{key} {} {} {} {}\n", r.x, r.y, r.w, r.h)).collect()
    }

    pub fn get(&self, key: &str) -> Option<LRect> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, r)| *r)
    }

    /// Keep `rect` under `key`, as the newest frame.
    pub fn put(&mut self, key: &str, rect: LRect) {
        self.entries.retain(|(k, _)| k != key);
        self.entries.push((key.to_owned(), rect));
        let extra = self.entries.len().saturating_sub(MAX_FRAMES);
        self.entries.drain(..extra);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// `<OctoSense home>/window-frames`.
pub fn path() -> PathBuf {
    crate::octosense::paths::home().join("window-frames")
}

pub fn load_from(path: &Path) -> Frames {
    std::fs::read_to_string(path).map(|text| Frames::parse(&text)).unwrap_or_default()
}

/// Replace the file whole (write, then rename): a reader never sees half of it.
pub fn save_to(path: &Path, frames: &Frames) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, frames.to_text())?;
    std::fs::rename(&tmp, path)
}

/// The frame kept under `key`, if any.
pub fn recall(key: &str) -> Option<LRect> {
    load_from(&path()).get(key)
}

/// Keep `rect` under `key` for the next run.
pub fn remember(key: &str, rect: LRect) {
    let path = path();
    let mut frames = load_from(&path);
    frames.put(key, rect);
    if let Err(error) = save_to(&path, &frames) {
        makepad_widgets::log!("wm: could not keep a window's frame in {}: {error}", path.display());
    }
}

/// A kept frame on today's desk: no larger than the room above the dock, no
/// smaller than the smallest window, and moved back onto the screen whole, as a
/// Mac puts a window saved on a display that is gone onto one that is there.
pub fn restored(saved: LRect, area: LRect, dock: f64) -> LRect {
    desktop_layout::resized(saved, 0.0, 0.0, area, dock)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(rect: LRect) -> DesktopWindow {
        DesktopWindow { client: 1, rect, minimized: false, hidden: false, maximized: false, fullscreen: false, snap: None, zoomed_from: None }
    }

    const AREA: LRect = LRect { x: 0.0, y: 30.0, w: 1400.0, h: 870.0 };

    #[test]
    fn an_app_resizes_only_its_floating_window_on_a_desktop() {
        let w = window(LRect::new(100.0, 80.0, 600.0, 400.0));
        assert!(may_resize(false, true, Some(&w)));
        assert!(!may_resize(true, true, Some(&w)), "a phone has no windows");
        assert!(!may_resize(false, false, Some(&w)), "tiles take the tiling's size");
        assert!(!may_resize(false, true, None), "no window");
        let changes: [fn(&mut DesktopWindow); 4] = [
            |w| w.maximized = true,
            |w| w.fullscreen = true,
            |w| w.minimized = true,
            |w| w.snap = Some(crate::snap::Zone::Left),
        ];
        for change in changes {
            let mut filled = w.clone();
            change(&mut filled);
            assert!(!may_resize(false, true, Some(&filled)), "{filled:?}");
        }
    }

    #[test]
    fn a_resize_sets_the_content_size_under_the_title_bar_and_stays_on_screen() {
        let r = LRect::new(100.0, 80.0, 600.0, 400.0);
        // 620 x 380 of content under a 32-point title bar; the top left stays.
        assert_eq!(resize_rect(r, 620.0, 380.0, 32.0, AREA, 0.0), Some(LRect::new(100.0, 80.0, 620.0, 412.0)));
        // Larger than the screen: the room above the dock, moved back in.
        let big = resize_rect(r, 5000.0, 5000.0, 32.0, AREA, 70.0).unwrap();
        assert_eq!(big, LRect::new(0.0, 30.0, 1400.0, 800.0));
        // Never below the smallest window.
        let small = resize_rect(r, 1.0, 1.0, 0.0, AREA, 0.0).unwrap();
        assert_eq!((small.w, small.h), (desktop_layout::MIN_W, desktop_layout::MIN_H));
        // A size that is not one changes nothing.
        for (w, h) in [(0.0, 300.0), (-5.0, 300.0), (f64::NAN, 300.0), (300.0, f64::INFINITY)] {
            assert_eq!(resize_rect(r, w, h, 32.0, AREA, 0.0), None);
        }
    }

    #[test]
    fn launch_or_focus_finds_the_window_started_with_the_same_arguments() {
        let slot = |app: &str, args: &[&str]| {
            let mut s = ClientSlot::module(0, app, app);
            s.launch_args = args.iter().map(|a| a.to_string()).collect();
            s
        };
        let mut clients = std::collections::HashMap::new();
        clients.insert(4, slot("terminal", &[]));
        clients.insert(7, slot("terminal", &["--settings"]));
        clients.insert(9, slot("terminal", &["--settings"]));
        clients.insert(5, slot("sheets", &["--settings"]));
        let settings = vec!["--settings".to_string()];
        let all = |_| true;
        assert_eq!(window_for("terminal", &settings, &clients, all), Some(7), "the oldest");
        assert_eq!(window_for("terminal", &[], &clients, all), Some(4));
        assert_eq!(window_for("terminal", &["--cwd".into(), "/tmp".into()], &clients, all), None);
        assert_eq!(window_for("browser", &settings, &clients, all), None);
        // Not windows: on its way out, a standby, a preview, a stopped one, one off the desk.
        clients.get_mut(&7).unwrap().closing = Some(1.0);
        assert_eq!(window_for("terminal", &settings, &clients, all), Some(9));
        let changes: [fn(&mut ClientSlot); 4] = [|s| s.warm = true, |s| s.is_preview = true, |s| s.pane = true, |s| s.stopped = true];
        for change in changes {
            let mut c = clients.clone_slots();
            change(c.get_mut(&9).unwrap());
            assert_eq!(window_for("terminal", &settings, &c, all), None);
        }
        assert_eq!(window_for("terminal", &settings, &clients, |id| id != 9), None);
    }

    trait CloneSlots {
        fn clone_slots(&self) -> std::collections::HashMap<ClientId, ClientSlot>;
    }

    impl CloneSlots for std::collections::HashMap<ClientId, ClientSlot> {
        fn clone_slots(&self) -> std::collections::HashMap<ClientId, ClientSlot> {
            self.iter()
                .map(|(id, s)| {
                    let mut copy = ClientSlot::module(*id, &s.app, &s.title);
                    copy.launch_args = s.launch_args.clone();
                    copy.closing = s.closing;
                    (*id, copy)
                })
                .collect()
        }
    }

    #[test]
    fn a_frame_is_kept_under_the_apps_own_id_and_a_plain_name() {
        assert_eq!(key("terminal", "w-1-2"), Some("terminal/w-1-2".into()));
        assert_eq!(key("hub:com.example", "main"), Some("hub:com.example/main".into()));
        assert_eq!(key("terminal", "../sheets/main"), None);
        assert_eq!(key("terminal", "a b"), None);
        assert_eq!(key("", "main"), None);
        assert_eq!(key("ter minal", "main"), None);
    }

    #[test]
    fn frames_round_trip_and_the_newest_are_kept() {
        let mut frames = Frames::default();
        frames.put("terminal/a", LRect::new(10.0, 40.5, 700.0, 480.0));
        frames.put("terminal/b", LRect::new(-30.0, 60.0, 500.0, 300.0));
        let again = Frames::parse(&frames.to_text());
        assert_eq!(again, frames);
        assert_eq!(again.get("terminal/a"), Some(LRect::new(10.0, 40.5, 700.0, 480.0)));
        // A frame put again is the newest, and only the last MAX_FRAMES stay.
        frames.put("terminal/a", LRect::new(1.0, 2.0, 3.0, 4.0));
        for i in 0..MAX_FRAMES {
            frames.put(&format!("app/f{i}"), LRect::new(0.0, 0.0, 100.0, 100.0));
        }
        assert_eq!(frames.len(), MAX_FRAMES);
        assert_eq!(frames.get("terminal/b"), None);
        assert_eq!(frames.get("terminal/a"), None);
        assert!(frames.get(&format!("app/f{}", MAX_FRAMES - 1)).is_some());
        // Lines that are not frames are skipped.
        let parsed = Frames::parse("garbage\nterminal/x 1 2 3\nterminal/y 1 2 -3 4\nterminal/z 1 2 NaN 4\nnoslash 1 2 3 4\nterminal/ok 1 2 3 4\n");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed.get("terminal/ok"), Some(LRect::new(1.0, 2.0, 3.0, 4.0)));
    }

    #[test]
    fn the_file_is_replaced_whole_and_read_back() {
        let dir = std::env::temp_dir().join(format!("octosense-frames-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("window-frames");
        assert_eq!(load_from(&path), Frames::default(), "no file: no frames");
        let mut frames = Frames::default();
        frames.put("terminal/main", LRect::new(120.0, 90.0, 760.0, 520.0));
        save_to(&path, &frames).unwrap();
        assert_eq!(load_from(&path).get("terminal/main"), Some(LRect::new(120.0, 90.0, 760.0, 520.0)));
        assert!(!path.with_extension("tmp").exists(), "no temporary file left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_kept_frame_comes_back_on_a_visible_display() {
        // Where it was, when that is still on screen.
        let kept = LRect::new(200.0, 100.0, 760.0, 520.0);
        assert_eq!(restored(kept, AREA, 0.0), kept);
        // Saved on a larger display, or off to the right: back on this one, whole.
        assert_eq!(restored(LRect::new(2500.0, 1400.0, 760.0, 520.0), AREA, 0.0), LRect::new(640.0, 380.0, 760.0, 520.0));
        assert_eq!(restored(LRect::new(-900.0, -50.0, 3000.0, 2000.0), AREA, 70.0), LRect::new(0.0, 30.0, 1400.0, 800.0));
    }
}
