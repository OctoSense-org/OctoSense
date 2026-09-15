//! The phone shell on makepad's frame monitor (`Cx::perf_monitor`), off
//! unless asked for.
//!
//! Switched on by `--test-action perf:on`, `OCTOSENSE_PERF=1`, or three
//! quick taps on the status bar's battery icon. While on:
//!
//! - the shell's surfaces are app channels of the monitor — `home`
//!   (wallpaper, home page, tile placeholders), `module` (hosted apps and
//!   tiles drawing into their captures), `glass` (backdrop snapshots, the
//!   blur pyramids, the compositor's finish), `overlay` (status bar, island,
//!   keyboard, navigation), `shade`, `groups` — next to the platform's own
//!   (`event`, and on macOS `draw`/`wait`/`gpu`), so the stacked plot shows
//!   where main-thread time goes and the gap strip shows pacing;
//! - the `PerfGraph` widget is drawn topmost in the phone shell
//!   (`PhoneSurface::draw_overlay`), on the frames the shell draws anyway —
//!   it never schedules frames of its own here, so an idle screen stays idle;
//! - every 2 s the shell's tick logs one `[perf]` line read back from the
//!   monitor's ring: frames, p50/p95/max gap, missed 16.7/33 ms frames, the
//!   per-channel cost per frame, and what asked for the frames (an
//!   animation, a finger, an action, or something outside the shell such
//!   as a hosted app redrawing itself).
//!
//! The line ends with a pass census: how many times the platform repainted
//! (`painted_serial` of the newest pass, which is the repaint id) against
//! how many phone scenes were drawn, and which attached passes the latest
//! repaint painted, sampled at every tick. A window that presents without
//! the shell drawing (a capture, blur or cache pass kept dirty from inside)
//! shows up there by name.
//!
//! The Android backend never closes the monitor's frames itself (only
//! macOS calls `frame_boundary`), so the shell does, at the start of every
//! phone scene draw. Only six app channels exist (`PERF_MONITOR_MAX_CHANNELS`
//! minus the built-ins): the island is inside `overlay`.
use makepad_widgets::*;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Why the shell asked for the frame that was drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// `PhoneState::step` still had something moving.
    Anim,
    /// A finger the recognizer owns (the frame keeps running for the hold).
    Gesture,
    /// A phone action or pointer event asked for a redraw.
    Action,
    /// Nothing in the shell asked: a hosted app redrew, or a platform event.
    External,
}
const REASONS: [Reason; 4] = [Reason::Anim, Reason::Gesture, Reason::Action, Reason::External];

/// The shell's app channels, registered once by name (indexes are stable).
#[derive(Clone, Copy, Debug)]
pub struct Channels {
    pub home: PerfChannel,
    pub module: PerfChannel,
    pub glass: PerfChannel,
    pub overlay: PerfChannel,
    pub shade: PerfChannel,
    pub groups: PerfChannel,
}

struct Collector {
    channels: Option<Channels>,
    reasons: [u32; 4],
    pending: Option<Reason>,
    /// `frames_painted` at the last report: the frames since are the new ones.
    reported_frames: u64,
    last_report: Option<Instant>,
    taps: Vec<f64>,
    /// The newest `painted_serial` at the last report, and which passes the
    /// latest repaint painted at each tick since (name, ticks seen hot).
    last_serial: u64,
    hot: Vec<(String, u32)>,
    census_ticks: u32,
    /// Events the app saw in the window: next-frame, signal, timer,
    /// actions, draw, other.
    events: [u32; 6],
    /// Timer events in the window by timer id.
    timers: Vec<(u64, u32)>,
    /// Tick samples of the backend's repaint flags: demo_time_repaint set,
    /// any pass paint_dirty, any repaint_requested, draw items owing an upload.
    flags: [u32; 4],
}

thread_local! {
    static COLLECTOR: RefCell<Collector> = RefCell::new(Collector {
        channels: None, reasons: [0; 4], pending: None, reported_frames: 0, last_report: None, taps: Vec::new(),
        last_serial: 0, hot: Vec::new(), census_ticks: 0, events: [0; 6], timers: Vec::new(), flags: [0; 4],
    });
}

/// How often a line is logged.
pub const REPORT_EVERY_S: f64 = 2.0;
/// Three taps on the battery icon inside this window switch the monitor.
/// The island's clock uses the same window: long enough for three
/// `adb shell input tap`s (each one is ~0.7 s of process start on a phone),
/// short enough that ordinary taps on the status bar never add up to three.
pub const TAP_WINDOW_S: f64 = 1.6;
/// A gap longer than this is the shell being idle (it draws on demand),
/// not a late frame: counted apart from the pacing quantiles.
pub const IDLE_GAP_MS: f32 = 500.0;

pub fn enabled() -> bool { ENABLED.load(Ordering::Relaxed) }

pub fn channels(cx: &mut Cx) -> Channels {
    COLLECTOR.with(|c| {
        let mut c = c.borrow_mut();
        *c.channels.get_or_insert_with(|| Channels {
            home: cx.perf_monitor.channel("home", 0x6aa9ff),
            module: cx.perf_monitor.channel("module", 0xff6ad5),
            glass: cx.perf_monitor.channel("glass", 0x4fd0c8),
            overlay: cx.perf_monitor.channel("overlay", 0xffe06a),
            shade: cx.perf_monitor.channel("shade", 0xc0c0ff),
            groups: cx.perf_monitor.channel("groups", 0x8aff6a),
        })
    })
}

pub fn set_enabled(cx: &mut Cx, on: bool) {
    let was = ENABLED.swap(on, Ordering::Relaxed);
    cx.perf_monitor.set_enabled(on);
    if on { channels(cx); }
    if was != on {
        log!("[perf] monitor {}", if on { "on" } else { "off" });
        COLLECTOR.with(|c| {
            let mut c = c.borrow_mut();
            c.reasons = [0; 4];
            c.pending = None;
            c.reported_frames = cx.perf_monitor.frames_painted();
            c.last_report = Some(Instant::now());
            c.last_serial = pass_rows(cx).iter().map(|r| r.1).max().unwrap_or(0);
            c.hot.clear();
            c.census_ticks = 0;
        });
    }
}

/// `OCTOSENSE_PERF=1` in the environment switches the monitor on at start.
pub fn init_from_env(cx: &mut Cx) {
    if std::env::var("OCTOSENSE_PERF").is_ok_and(|v| !v.is_empty() && v != "0") { set_enabled(cx, true); }
}

/// A tap on the battery icon at `now` (seconds): three within
/// `TAP_WINDOW_S` toggle the monitor. True when this tap toggled it.
pub fn battery_tap(cx: &mut Cx, now: f64) -> bool {
    let third = COLLECTOR.with(|c| {
        let mut c = c.borrow_mut();
        c.taps.retain(|t| now - *t < TAP_WINDOW_S);
        c.taps.push(now);
        if c.taps.len() >= 3 { c.taps.clear(); true } else { false }
    });
    if third { set_enabled(cx, !enabled()); }
    third
}

/// The shell asked for the next frame for this reason. The strongest
/// reason wins when several ask before the frame is drawn.
pub fn asked(reason: Reason) {
    if !enabled() { return; }
    COLLECTOR.with(|c| {
        let mut c = c.borrow_mut();
        c.pending = Some(match (c.pending, reason) {
            (Some(Reason::Gesture), _) | (_, Reason::Gesture) => Reason::Gesture,
            (Some(Reason::Anim), _) | (_, Reason::Anim) => Reason::Anim,
            _ => reason,
        });
    });
}

/// Every event the app handles, counted by kind while the monitor is on.
pub fn saw_event(event: &Event) {
    if !enabled() { return; }
    let i = match event {
        Event::NextFrame(_) => 0,
        Event::Signal => 1,
        Event::Timer(_) => 2,
        Event::Actions(_) => 3,
        Event::Draw(_) => 4,
        _ => 5,
    };
    COLLECTOR.with(|c| {
        let mut c = c.borrow_mut();
        c.events[i] += 1;
        if let Event::Timer(te) = event {
            match c.timers.iter_mut().find(|(id, _)| *id == te.timer_id) {
                Some((_, n)) => *n += 1,
                None => c.timers.push((te.timer_id, 1)),
            }
        }
    });
}

/// The phone scene starts drawing: close the monitor's frame (the backend
/// does not on Android) and account for who asked.
pub fn frame_boundary(cx: &mut Cx) {
    if !enabled() { return; }
    #[cfg(not(target_os = "macos"))]
    {
        let time = cx.seconds_since_app_start();
        cx.perf_monitor.frame_boundary(time);
    }
    #[cfg(target_os = "macos")]
    let _ = &cx;
    COLLECTOR.with(|c| {
        let mut c = c.borrow_mut();
        let reason = c.pending.take().unwrap_or(Reason::External);
        c.reasons[REASONS.iter().position(|r| *r == reason).unwrap()] += 1;
    });
}

/// `since` until now went to `channel`.
pub fn span(cx: &mut Cx, channel: PerfChannel, since: Instant) {
    if !enabled() { return; }
    cx.perf_monitor.add(channel, since.elapsed().as_micros() as u64);
}

/// Called from the shell's 1 s tick: every `REPORT_EVERY_S` one line from
/// the monitor's ring, and the counters start over.
pub fn tick(cx: &mut Cx) {
    if !enabled() { return; }
    let rows = pass_rows(cx);
    let line = COLLECTOR.with(|c| {
        let mut c = c.borrow_mut();
        let newest = rows.iter().map(|r| r.1).max().unwrap_or(0);
        for name in hot_passes(&rows, newest) {
            match c.hot.iter_mut().find(|(n, _)| *n == name) {
                Some((_, count)) => *count += 1,
                None => c.hot.push((name, 1)),
            }
        }
        c.census_ticks += 1;
        let (demo, dirty, requested, uploads) = backend_flags(cx);
        c.flags[0] += demo as u32;
        c.flags[1] += dirty;
        c.flags[2] += requested;
        c.flags[3] += uploads;
        let window = c.last_report.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
        if window < REPORT_EVERY_S { return None; }
        let painted = cx.perf_monitor.frames_painted();
        let new = (painted - c.reported_frames).min(PERF_MONITOR_HISTORY as u64) as usize;
        let mut ring = Vec::new();
        cx.perf_monitor.read(&mut ring);
        let frames = &ring[ring.len().saturating_sub(new)..];
        let names: Vec<String> = cx.perf_monitor.channels().iter().map(|ch| ch.name.clone()).collect();
        let mut line = report_line(window, frames, &names, c.reasons);
        line.push_str(&census_text(newest.saturating_sub(c.last_serial), new, &c.hot, c.census_ticks));
        let f = c.flags;
        line.push_str(&format!(" | flags over {} ticks: demo_time={} dirty_passes={} repaint_requested={} upload_pending_items={}", c.census_ticks, f[0], f[1], f[2], f[3]));
        c.flags = [0; 4];
        let terms = cx.draw_lists.instance_retirement_terms();
        if !terms.is_empty() { line.push_str(&format!(" | retirement-debt: {}", terms)); }
        let ts = time_shaders(cx);
        if !ts.is_empty() { line.push_str(&format!(" | time-shaders: {}", ts.join(" "))); }
        let e = c.events;
        line.push_str(&format!(" | events: next={} signal={} timer={} actions={} draw={} other={}", e[0], e[1], e[2], e[3], e[4], e[5]));
        c.events = [0; 6];
        let mut timers = std::mem::take(&mut c.timers);
        timers.sort_by(|a, b| b.1.cmp(&a.1));
        if !timers.is_empty() {
            line.push_str(" | timers:");
            for (id, n) in timers.iter().take(5) { line.push_str(&format!(" #{}×{}", id, n)); }
        }
        c.last_serial = newest;
        c.hot.clear();
        c.census_ticks = 0;
        c.reasons = [0; 4];
        c.reported_frames = painted;
        c.last_report = Some(Instant::now());
        Some(line)
    });
    if let Some(line) = line { log!("{}", line); }
}

/// Every pass: its name (with its slot, and `-` when detached), the repaint
/// that last painted it, and whether it is attached to a consumer.
fn pass_rows(cx: &Cx) -> Vec<(String, u64, bool)> {
    cx.passes.id_iter().map(|id| {
        let pass = &cx.passes[id];
        let attached = !matches!(pass.parent, CxDrawPassParent::None);
        let kind = match pass.parent {
            CxDrawPassParent::Window(_) => "window",
            CxDrawPassParent::DrawPass(_) => "child",
            CxDrawPassParent::Xr => "xr",
            CxDrawPassParent::None => "-",
        };
        let name = if pass.debug_name.is_empty() { "pass" } else { pass.debug_name.as_str() };
        (format!("{name}#{:?}/{kind}", id), pass.painted_serial, attached)
    }).collect()
}

/// Draw calls whose shader reads the pass time (`uses_time`): one of them
/// on screen makes the GL/Metal backends repaint every live pass at display
/// rate (`demo_time_repaint`). Listed by shader, with the pass and how many
/// calls, for the lists recorded in the root's latest redraw.
fn time_shaders(cx: &Cx) -> Vec<String> {
    let mut out: Vec<(String, u32)> = Vec::new();
    for list_id in cx.draw_lists.id_iter() {
        let list = &cx.draw_lists[list_id];
        for i in 0..list.draw_items.len() {
            let Some(call) = list.draw_items[i].kind.draw_call() else { continue };
            let Some(sh) = cx.draw_shaders.shaders.get(call.draw_shader_id.index) else { continue };
            if !sh.mapping.uses_time { continue; }
            let key = format!("{:?}@{:?}", sh.debug_id, list.draw_pass_id);
            match out.iter_mut().find(|(k, _)| *k == key) {
                Some((_, n)) => *n += 1,
                None => out.push((key, 1)),
            }
        }
    }
    out.into_iter().map(|(k, n)| format!("{k}×{n}")).collect()
}

/// The backend's own reasons to repaint, sampled now: the time-repaint
/// flag, passes still dirty, passes asked for by name, and draw items that
/// owe the backend another upload frame.
fn backend_flags(cx: &Cx) -> (bool, u32, u32, u32) {
    let mut dirty = 0;
    let mut requested = 0;
    for id in cx.passes.id_iter() {
        let pass = &cx.passes[id];
        if matches!(pass.parent, CxDrawPassParent::None) { continue; }
        dirty += pass.paint_dirty as u32;
        requested += pass.repaint_requested as u32;
    }
    let mut uploads = 0;
    for list_id in cx.draw_lists.id_iter() {
        let list = &cx.draw_lists[list_id];
        for i in 0..list.draw_items.len() {
            uploads += list.draw_items[i].instance_upload_pending as u32;
        }
    }
    // Freed draw items still waiting for a worker to retire their backing:
    // while any are pending the GL backend sets `demo_time_repaint` on
    // every render (opengl.rs `retire_free_items`), Metal keeps its loop up.
    let retiring = cx.draw_lists.has_pending_instance_retirements() as u32;
    (cx.demo_time_repaint, dirty, requested, uploads + retiring * 1_000_000)
}

/// The attached passes the latest repaint (`newest`) painted.
pub fn hot_passes(rows: &[(String, u64, bool)], newest: u64) -> Vec<String> {
    if newest == 0 { return Vec::new(); }
    rows.iter().filter(|(_, serial, attached)| *attached && *serial == newest).map(|(name, _, _)| name.clone()).collect()
}

/// ` | repaints=R scenes=S | hot: name×k/n …` — the platform's repaints in
/// the window against the phone scenes drawn, and how often each pass was
/// in the latest repaint across `ticks` samples.
pub fn census_text(repaints: u64, scenes: usize, hot: &[(String, u32)], ticks: u32) -> String {
    let mut s = format!(" | repaints={} scenes={}", repaints, scenes);
    if !hot.is_empty() {
        let mut hot = hot.to_vec();
        hot.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        s.push_str(" | hot:");
        for (name, count) in hot.iter().take(8) {
            s.push_str(&format!(" {}×{}/{}", name, count, ticks));
        }
    }
    s
}

/// p50, p95 and max of `v` (zeros when empty).
pub fn quantiles(v: &[f32]) -> (f32, f32, f32) {
    if v.is_empty() { return (0.0, 0.0, 0.0); }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |f: f64| s[((s.len() as f64 * f) as usize).min(s.len() - 1)];
    (q(0.5), q(0.95), *s.last().unwrap())
}

/// The `[perf]` line for `frames` (the ring's newest, oldest first) drawn
/// in a window of `window_s`. `names` are the monitor's channel names in
/// index order; `reasons` count anim/gesture/action/external.
pub fn report_line(window_s: f64, frames: &[PerfMonitorFrame], names: &[String], reasons: [u32; 4]) -> String {
    // The first frame after enabling has no gap; an idle pause is not a late frame.
    let gaps: Vec<f32> = frames.iter().map(|f| f.gap_ms).filter(|g| *g > 0.0 && *g <= IDLE_GAP_MS).collect();
    let idle = frames.iter().filter(|f| f.gap_ms > IDLE_GAP_MS).count();
    let (p50, p95, max) = quantiles(&gaps);
    let over16 = gaps.iter().filter(|g| **g > 17.5).count();
    let over33 = gaps.iter().filter(|g| **g > 34.0).count();
    let mut s = format!(
        "[perf] {:.1}s: frames={} ({:.1}/s) gap p50={:.1} p95={:.1} max={:.1} ms >16.7={} >33={} idle-gaps={}",
        window_s, frames.len(), frames.len() as f64 / window_s.max(1e-6), p50, p95, max, over16, over33, idle,
    );
    if !frames.is_empty() {
        s.push_str(" | ms/frame:");
        for (i, name) in names.iter().enumerate().take(PERF_MONITOR_MAX_CHANNELS) {
            let total: u64 = frames.iter().map(|f| f.channel_us[i] as u64).sum();
            if total == 0 { continue; }
            let max = frames.iter().map(|f| f.channel_us[i]).max().unwrap_or(0);
            s.push_str(&format!(" {}={:.2}/{:.2}", name, total as f64 / frames.len() as f64 / 1000.0, max as f64 / 1000.0));
        }
    }
    s.push_str(&format!(
        " | asked by: anim={} gesture={} action={} external={}",
        reasons[0], reasons[1], reasons[2], reasons[3]
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(gap_ms: f32, us: &[(usize, u32)]) -> PerfMonitorFrame {
        let mut f = PerfMonitorFrame { gap_ms, ..Default::default() };
        for (i, v) in us { f.channel_us[*i] = *v; }
        f
    }

    #[test]
    fn quantiles_pick_the_middle_the_tail_and_the_worst() {
        let v: Vec<f32> = (1..=100).map(|x| x as f32).collect();
        assert_eq!(quantiles(&v), (51.0, 96.0, 100.0));
        assert_eq!(quantiles(&[]), (0.0, 0.0, 0.0));
    }

    #[test]
    fn the_line_counts_missed_frames_idle_pauses_channels_and_reasons() {
        let names: Vec<String> = ["event", "script", "gc", "draw", "wait", "gpu", "home", "module"].iter().map(|s| s.to_string()).collect();
        let frames = [
            frame(0.0, &[(0, 500), (6, 1000)]),      // first frame after enabling: no gap
            frame(16.6, &[(0, 500), (6, 3000)]),
            frame(20.0, &[(0, 500), (6, 2000), (7, 4000)]),
            frame(40.0, &[(0, 500)]),
            frame(1200.0, &[(0, 500)]),              // an idle pause, not a late frame
        ];
        let line = report_line(2.0, &frames, &names, [1, 0, 2, 2]);
        assert!(line.starts_with("[perf] 2.0s: frames=5 (2.5/s) gap p50=20.0 p95=40.0 max=40.0 ms >16.7=2 >33=1 idle-gaps=1"), "{line}");
        assert!(line.contains(" event=0.50/0.50 home=1.20/3.00 module=0.80/4.00"), "{line}");
        assert!(!line.contains("script="), "silent channels are left out: {line}");
        assert!(line.ends_with("asked by: anim=1 gesture=0 action=2 external=2"), "{line}");
    }

    #[test]
    fn the_census_names_attached_passes_the_latest_repaint_painted() {
        let rows = vec![
            ("main#DrawPassId(0)/window".to_string(), 120, true),
            ("pass#DrawPassId(3)/child".to_string(), 120, true),
            ("wm_phone_capture#DrawPassId(7)/-".to_string(), 120, false), // detached: not a consumer
            ("gauss_scene#DrawPassId(9)/child".to_string(), 64, true),     // painted earlier
        ];
        assert_eq!(hot_passes(&rows, 120), vec!["main#DrawPassId(0)/window".to_string(), "pass#DrawPassId(3)/child".to_string()]);
        assert!(hot_passes(&rows, 0).is_empty());
        let text = census_text(108, 2, &[("pass#3/child".into(), 2), ("main#0/window".into(), 2), ("x#1/child".into(), 1)], 2);
        assert_eq!(text, " | repaints=108 scenes=2 | hot: main#0/window×2/2 pass#3/child×2/2 x#1/child×1/2");
        assert_eq!(census_text(0, 0, &[], 2), " | repaints=0 scenes=0");
    }

    #[test]
    fn an_empty_window_still_reports_zero_frames() {
        let line = report_line(2.0, &[], &[], [0; 4]);
        assert!(line.starts_with("[perf] 2.0s: frames=0 (0.0/s) gap p50=0.0"), "{line}");
        assert!(!line.contains("ms/frame"), "{line}");
    }

    #[test]
    fn three_quick_taps_toggle_and_slow_ones_do_not() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        // ENABLED is one flag for the process; this test owns it.
        set_enabled(&mut cx, false);
        assert!(!battery_tap(&mut cx, 10.0) && !battery_tap(&mut cx, 10.2));
        assert!(battery_tap(&mut cx, 10.5) && enabled() && cx.perf_monitor.enabled());
        assert!(!battery_tap(&mut cx, 20.0) && !battery_tap(&mut cx, 22.0) && !battery_tap(&mut cx, 24.0));
        // Three adb taps, ~0.72 s apart, land inside the window.
        assert!(!battery_tap(&mut cx, 40.0) && !battery_tap(&mut cx, 40.72) && battery_tap(&mut cx, 41.44) && !enabled());
        assert!(!battery_tap(&mut cx, 50.0) && !battery_tap(&mut cx, 50.72) && battery_tap(&mut cx, 51.44) && enabled());
        assert!(enabled());
        assert!(!battery_tap(&mut cx, 30.0) && !battery_tap(&mut cx, 30.1) && battery_tap(&mut cx, 30.2));
        assert!(!enabled() && !cx.perf_monitor.enabled());
    }
}
