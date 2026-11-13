//! `shell/plugins/bar/` — the top bar.
//!
//! The module list and its order are `config/omarchy/shell.json`:
//! left `[menu, workspaces]`, center `[indicators, clock, keyboard-layout,
//! weather, system-update]` with `centerAnchor: omarchy.clock`, right
//! `[tray, agents, bluetooth, network, audio, monitor, power]`.
//!
//! Geometry from `Bar.qml` + `Ui/WidgetButton.qml` / `BarIconButton.qml`:
//! the strip is `Style.bar.sizeHorizontal` (26) tall and filled with
//! `Color.bar.background` at α 1.0 — no border, no separators; the left and
//! right clusters sit `space(8)` off their edge, modules inside a cluster
//! touch (`Row{spacing: 0}`) and pad themselves; the center anchor module
//! is centered on the SCREEN with the modules before it flush against its
//! left edge and the ones after flush against its right. Icon buttons are a
//! 27px slot around a 16px canvas, indicators a 21px status slot at
//! `caption` 10, the clock a label with 8.75px side margins, workspace
//! pills 20 wide with 1px between them, and the module whose panel is open
//! wears a 2px accent pill inset 2px from the bar's inner edge, 15 long.
//!
//! Mouse: a press on the menu button opens the menu, a press on a
//! workspace focuses it, a press on a tray/status module opens its panel,
//! and the wheel over audio/monitor steps volume/brightness — the same
//! gestures as the original.

use makepad_widgets::*;

use super::ui::{contains, rect, DrawShellFill, Ico, ShellDraw};
use super::{alpha, fade, MaterialTokens, ShellTokens};

/// Every clickable thing in the bar, in `shell.json` id terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarModule {
    Menu,
    Style,
    Appearance,
    Workspace(usize),
    /// A Mac's one item for its desktops, while there are several: the current one's
    /// number, and a menu of them.
    Desktops,
    /// `widgets/ActiveWindow.qml` — not in the stock `shell.json` center
    /// list, but our bar carries it right after the workspaces.
    ActiveWindow,
    /// "Ask <app>" beside the active window's title, when the focused app
    /// has an agent (ADR 0004 §4): opens the shell's panel for it.
    AskAgent,
    Indicator(usize),
    /// The glance panel (F9): the cards apps published, their count on a
    /// badge, lit while some are new since the panel was last open.
    Glance,
    Clock,
    KeyboardLayout,
    Weather,
    SystemUpdate,
    Tray(usize),
    Bluetooth,
    Network,
    Audio,
    Monitor,
    Power,
    /// The window's own controls, at the far right where Windows and most
    /// Linux desktops put them: the omarchy bar IS this window's caption,
    /// and a client-sized frame draws none of its own. macOS keeps its
    /// traffic lights on the left instead.
    WindowMin,
    WindowMax,
    WindowClose,
    /// A Mac's status item while a time box runs (`approvals::time_boxes`): its
    /// menu says what is approved and for how long, with Stop.
    TimeBox,
    /// A menu title on a Mac's menu bar: File, Edit, View, Window, Help.
    MacMenu(usize),
    /// A Mac's own items at the bar's right end, beside the clock: Spotlight, and
    /// Control Center with its modules.
    Spotlight,
    ControlCenter,
}

/// One workspace pill.
#[derive(Clone, Debug)]
pub struct WorkspaceCell {
    pub label: String,
    pub occupied: bool,
    pub focused: bool,
}

/// A bar indicator (`plugins/bar/indicators/`): one glyph with an active
/// and an inactive reading. Inactive ones are hidden until the pointer is
/// over the indicator block, then shown at α .45.
#[derive(Clone, Debug)]
pub struct Indicator {
    pub icon: Ico,
    pub active_icon: Ico,
    pub active: bool,
    pub tooltip: &'static str,
}

/// What the bar shows. The WM fills it from real state; the gallery fills
/// it with fixtures. Nothing in here is sampled by the widget itself.
#[derive(Clone, Debug, Default)]
pub struct BarData {
    pub style: crate::desktop::DesktopStyle,
    pub dark: bool,
    pub workspaces: Vec<WorkspaceCell>,
    pub indicators: Vec<Indicator>,
    /// The focused window's title (`ActiveWindow.qml`).
    pub active_window: Option<String>,
    /// The focused app's name: a Mac's menu bar shows it in bold.
    pub active_app: Option<String>,
    /// The focused app's name, when it has an agent: the bar offers
    /// "Ask <name>".
    pub ask_agent: Option<String>,
    /// Already formatted — `dddd HH:mm`, or the alt `d MMMM 'W'ww yyyy`.
    pub clock: String,
    /// `sample_mac_clock`, for a Mac-family bar; empty uses `clock`.
    pub mac_clock: String,
    /// A Mac bar's menus after the app's name: the app in front can add its own.
    pub mac_titles: Vec<String>,
    pub keyboard_layout: Option<String>,
    pub weather: Option<String>,
    pub system_update: bool,
    pub tray: Vec<Ico>,
    /// `None` renders the module in its "unavailable" reading: the icon at
    /// α .45, which is what an omarchy indicator does when its service is
    /// not there.
    pub bluetooth: Option<bool>,
    pub network: Option<bool>,
    /// The Mac's Wi-Fi, where it has one ([`sample_wifi`]).
    pub wifi: Option<Wifi>,
    pub volume: Option<u32>,
    pub muted: bool,
    pub brightness: Option<u32>,
    pub battery: Option<Battery>,
    /// The module whose panel is open — it wears the accent pill.
    pub open_panel: Option<BarModule>,
    /// The window is maximized: the middle control shows "restore".
    pub maximized: bool,
    /// The glance screen's cards and how many arrived since the panel was
    /// last open (`None`: no glance button).
    pub glance: Option<(usize, usize)>,
    /// A Mac's running time box: the minutes left of the one that ends first.
    pub time_box: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Battery {
    pub percent: u32,
    pub charging: bool,
}

/// The Mac's Wi-Fi, as far as an app may read it. macOS keeps the names of
/// networks from an app without Location access, so the joined one is known
/// by its link and its lock alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wifi {
    /// Its device (`en0`), which turns it on and off.
    pub device: String,
    pub on: bool,
    /// Joined to a network.
    pub joined: bool,
    /// The joined network asks for a password.
    pub secure: bool,
}

impl BarData {
    /// The fixture the gallery (and `plugins/dev-gallery`) draws with.
    pub fn fixture() -> Self {
        Self {
            style: Default::default(),
            dark: false,
            workspaces: vec![
                WorkspaceCell {
                    label: "1".into(),
                    occupied: true,
                    focused: true,
                },
                WorkspaceCell {
                    label: "2".into(),
                    occupied: true,
                    focused: false,
                },
                WorkspaceCell {
                    label: "3".into(),
                    occupied: false,
                    focused: false,
                },
                WorkspaceCell {
                    label: "4".into(),
                    occupied: false,
                    focused: false,
                },
                WorkspaceCell {
                    label: "5".into(),
                    occupied: false,
                    focused: false,
                },
            ],
            indicators: default_indicators(),
            active_window: Some("terminal — ~/makepad".into()),
            active_app: Some("Terminal".into()),
            ask_agent: None,
            clock: "Thursday 21:34".into(),
            mac_clock: "Thu Oct 1  21:34".into(),
            mac_titles: ["File", "Edit", "View", "Window", "Help"].map(String::from).to_vec(),
            keyboard_layout: Some("en".into()),
            weather: None,
            system_update: false,
            tray: vec![],
            bluetooth: Some(true),
            network: Some(true),
            wifi: None,
            volume: Some(62),
            muted: false,
            brightness: Some(80),
            battery: Some(Battery {
                percent: 87,
                charging: true,
            }),
            open_panel: None,
            maximized: false,
            glance: None,
            time_box: None,
        }
    }
}

/// `defaultIndicatorEntries` — Dictation, ScreenRecording, Reminder,
/// NightLight, Dnd, StayAwake — with the ones we can actually mean.
pub fn default_indicators() -> Vec<Indicator> {
    vec![
        Indicator {
            icon: Ico::Record,
            active_icon: Ico::Record,
            active: false,
            tooltip: "Screen Recording",
        },
        Indicator {
            icon: Ico::Moon,
            active_icon: Ico::Moon,
            active: false,
            tooltip: "Night Light",
        },
        Indicator {
            icon: Ico::Bell,
            active_icon: Ico::BellOff,
            active: false,
            tooltip: "Silence Notifications",
        },
    ]
}

/// The volume glyph ladder: muted, then ≤33 / ≤66 / above.
pub fn volume_icon(level: Option<u32>, muted: bool) -> Ico {
    match level {
        _ if muted => Ico::Volume0,
        None => Ico::Volume0,
        Some(0) => Ico::Volume0,
        Some(v) if v <= 33 => Ico::Volume1,
        Some(v) if v <= 66 => Ico::Volume2,
        Some(_) => Ico::Volume3,
    }
}

// ======================================================================
// Cheap real data (macOS)
// ======================================================================

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (cmd, args);
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let out = std::process::Command::new(cmd).args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8(out.stdout).ok()
    }
}

/// `date +"%A %H:%M"` — omarchy's `dddd HH:mm` — formatted in-process from
/// the C library's local time (`localtime_r` + `strftime`, the same tz data
/// `date` reads). Forking `date` twice a second cost an idle phone 11% of a
/// core on this thread alone.
pub fn sample_clock(alt: bool) -> String {
    local_time(if alt { c"%-d %B W%V %Y" } else { c"%A %H:%M" })
}

/// Whether a Mac-family bar's clock writes 24 hours. A Mac follows its own setting,
/// which the status sampler reads; a hidden test shell stays on 12 hours.
static MAC_CLOCK_24_HOUR: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(!cfg!(target_os = "macos"));

/// Read the Mac's Date & Time setting: AppleScript writes a time in it, with a
/// marker for the half of the day (`PM`, `下午`) only on a 12-hour Mac.
#[cfg(target_os = "macos")]
fn sample_mac_hour_cycle() {
    if crate::launch_input::hidden_windows() {
        return;
    }
    if let Some(time) = run("osascript", &["-e", "time string of (current date)"]) {
        let hours_24 = !time.chars().any(char::is_alphabetic);
        MAC_CLOCK_24_HOUR.store(hours_24, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Whether the Mac is in its dark appearance. A test shell's stays what its
/// test asked for, whatever this machine is set to.
pub fn sample_mac_dark() -> Option<bool> {
    #[cfg(target_os = "macos")]
    if !crate::launch_input::hidden_windows() {
        // The key exists only while the Mac is dark.
        return Some(run("defaults", &["read", "-g", "AppleInterfaceStyle"]).is_some_and(|style| style.trim() == "Dark"));
    }
    None
}

/// The clock as a Mac's menu bar shows it: `Mon Nov 2  9:41 PM`, or `Mon Nov 2  21:41`
/// on a Mac set to 24 hours.
pub fn sample_mac_clock() -> String {
    if MAC_CLOCK_24_HOUR.load(std::sync::atomic::Ordering::Relaxed) {
        local_time(c"%a %b %-d  %H:%M")
    } else {
        local_time(c"%a %b %-d  %-I:%M %p")
    }
}

/// Now, in local time, formatted by `strftime`'s `fmt` (`%H:%M`): the C
/// library's `localtime_r` on Unix, `date` elsewhere.
pub fn local_time(fmt: &std::ffi::CStr) -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 64];
        let written = unsafe {
            let now = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&now, &mut tm).is_null() {
                return String::new();
            }
            libc::strftime(buf.as_mut_ptr() as *mut libc::c_char, buf.len(), fmt.as_ptr(), &tm)
        };
        String::from_utf8_lossy(&buf[..written]).trim().to_string()
    }
    #[cfg(not(unix))]
    {
        let fmt = format!("+{}", fmt.to_str().unwrap_or_default());
        run("date", &[&fmt])
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }
}

/// Keep Splash's `std.local_time` on this device's zone. makepad has no
/// timezone database: it shows UTC until its host sets the offset
/// (`set_script_local_utc_offset_secs`). The same local time as the clock,
/// refreshed with it, so a daylight-saving change or a new zone follows.
pub fn refresh_script_utc_offset() {
    if let Some(offset) = local_utc_offset_secs() {
        makepad_widgets::makepad_platform::script::timer::set_script_local_utc_offset_secs(offset);
    }
}

/// This device's offset from UTC now, in seconds: what `localtime_r` gives
/// the clock on Unix.
fn local_utc_offset_secs() -> Option<i64> {
    #[cfg(unix)]
    {
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let now = unsafe { libc::time(std::ptr::null_mut()) };
        if unsafe { libc::localtime_r(&now, &mut tm) }.is_null() {
            return None;
        }
        Some(tm.tm_gmtoff as i64)
    }
    #[cfg(not(unix))]
    {
        Some(chrono::Local::now().offset().local_minus_utc() as i64)
    }
}

/// Everything the bar reads from the OS, gathered OFF the main thread.
///
/// Every sampler below is a `fork+exec+wait` (`osascript`, `date`, `pmset`,
/// `route`, `defaults`) that can take hundreds of milliseconds under load.
/// Running them on the main thread's 1s status timer blocked the whole
/// event loop ~0.5s every second — which starved the 8ms Ticks that drive
/// every hosted tile, freezing all child apps in visible hiccups (the
/// "0.5s of nothing while dragging" report; a `sample` showed 614/1646
/// main-thread samples inside update_status, 326 in sample_volume).
#[derive(Clone, Default)]
pub struct SampledStatus {
    pub volume: Option<u32>,
    pub muted: bool,
    pub clock: String,
    pub clock_alt: String,
    pub battery: Option<Battery>,
    pub network: Option<bool>,
    pub bluetooth: Option<bool>,
    pub wifi: Option<Wifi>,
    /// The Mac's own appearance; never read for a hidden test shell.
    pub mac_dark: Option<bool>,
}

/// Spawn the lifetime sampler worker: refreshes the cheap facts every second
/// and the slow ones every fifth round, then sends an immutable snapshot to
/// the UI. Dropping the receiver ends the worker.
pub fn start_status_sampler(
    spawner: &makepad_widgets::makepad_platform::thread::ThreadSpawner,
) -> Result<
    (
        std::sync::mpsc::Receiver<SampledStatus>,
        makepad_widgets::makepad_platform::thread::TaskHandle<()>,
    ),
    makepad_widgets::makepad_platform::thread::SpawnError,
> {
    use makepad_widgets::makepad_platform::thread::{CancellationToken, SignalToUI, ThreadOptions};
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = spawner.spawn_worker(
        ThreadOptions {
            name: Some("wm-status".into()),
            ..Default::default()
        },
        move || {
            let mut round: u32 = 0;
            let mut status = SampledStatus::default();
            let wait = CancellationToken::new();
            // System Profiler can be slow or wait on a hardware service. Keep
            // that read off the clock/status worker as well as off the UI.
            #[cfg(target_os = "macos")]
            let (bluetooth_tx, bluetooth_rx) = std::sync::mpsc::channel();
            #[cfg(target_os = "macos")]
            let mut bluetooth_pending = false;
            // Each `osascript` wakes the permission and audio daemons, so the
            // volume is one of the slow facts: the shell's own changes show at
            // once, a change made elsewhere within five seconds. Nobody sees a
            // hidden test shell's bar, so it asks once a minute.
            let slow_every: u32 = if crate::launch_input::hidden_windows() { 60 } else { 5 };
            loop {
                #[cfg(target_os = "macos")]
                if let Ok(bluetooth) = bluetooth_rx.try_recv() {
                    status.bluetooth = bluetooth;
                    bluetooth_pending = false;
                }
                status.clock = sample_clock(false);
                status.clock_alt = sample_clock(true);
                refresh_script_utc_offset();
                #[cfg(target_os = "macos")]
                if round % 60 == 0 {
                    sample_mac_hour_cycle();
                }
                if round % slow_every == 0 {
                    let (volume, muted) = sample_volume();
                    status.volume = volume;
                    status.muted = muted;
                    status.battery = sample_battery();
                    status.network = sample_network();
                    status.wifi = sample_wifi();
                    #[cfg(target_os = "macos")]
                    if !bluetooth_pending {
                        let tx = bluetooth_tx.clone();
                        bluetooth_pending = std::thread::Builder::new()
                            .name("wm-bluetooth".into())
                            .spawn(move || { let _ = tx.send(sample_bluetooth()); })
                            .is_ok();
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        status.bluetooth = sample_bluetooth();
                    }
                    status.mac_dark = sample_mac_dark();
                }
                if tx.send(status.clone()).is_err() {
                    break;
                }
                SignalToUI::set_ui_signal();
                round = round.wrapping_add(1);
                let _ = wait.wait_until(makepad_widgets::Cx::monotonic_now() + 1.0);
            }
        },
    )?;
    Ok((rx, worker))
}

/// Output volume and mute, from the shared system mixer.
pub fn sample_volume() -> (Option<u32>, bool) {
    #[cfg(target_os = "macos")]
    {
        let level = run(
            "osascript",
            &["-e", "output volume of (get volume settings)"],
        )
        .and_then(|s| s.trim().parse::<u32>().ok());
        let muted = run("osascript", &["-e", "output muted of (get volume settings)"])
            .map(|s| s.trim() == "true")
            .unwrap_or(false);
        (level, muted)
    }
    #[cfg(not(target_os = "macos"))]
    {
        (None, false)
    }
}

/// `pmset -g batt` — percent plus whether it is on its adapter.
pub fn sample_battery() -> Option<Battery> {
    #[cfg(target_os = "macos")]
    {
        parse_battery(&run("pmset", &["-g", "batt"])?)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// What `pmset -g batt` says. Its first line names the source: "discharging" holds "charging".
#[cfg(any(target_os = "macos", test))]
fn parse_battery(out: &str) -> Option<Battery> {
    let percent = out
        .split('\t')
        .nth(1)
        .or_else(|| out.split(';').next())
        .and_then(|s| s.split('%').next())
        .and_then(|s| s.rsplit(|c: char| !c.is_ascii_digit()).next())
        .and_then(|s| s.parse::<u32>().ok())?;
    Some(Battery { percent, charging: out.contains("'AC Power'") })
}

/// Whether we have a default route with an address on it.
pub fn sample_network() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        match lookup_default_route(&["-n", "get", "default"])? {
            // No IPv4 route alone does not establish that a device with an
            // IPv6 default route is offline. This sampler cannot yet confirm
            // an IPv6 address, so keep that case unknown.
            None => match lookup_default_route(&["-n", "get", "-inet6", "default"])? {
                None => Some(false),
                Some(_) => None,
            },
            // A failed or empty address read does not rule out IPv6 or an
            // inaccessible interface, so it must not be presented as offline.
            Some(dev) => run("ipconfig", &["getifaddr", &dev])
                .filter(|address| !address.trim().is_empty())
                .map(|_| true),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn lookup_default_route(args: &[&str]) -> Option<Option<String>> {
    // `route get` can exit successfully when the default is absent and
    // report "not in table" only on stderr. Other failures stay unknown.
    let route = std::process::Command::new("route")
        .env("LC_ALL", "C")
        .args(args)
        .output().ok()?;
    let out = std::str::from_utf8(&route.stdout).ok()?;
    let err = std::str::from_utf8(&route.stderr).ok()?;
    default_route_device(out, err, route.status.success()).map(|device| device.map(str::to_owned))
}

/// Outer None means unknown; Some(None) means the default route is absent.
#[cfg(any(target_os = "macos", test))]
fn default_route_device<'a>(out: &'a str, err: &str, success: bool) -> Option<Option<&'a str>> {
    let device = out.lines()
        .find_map(|line| line.trim().strip_prefix("interface:"))
        .map(str::trim)
        .filter(|name| !name.is_empty());
    if err.trim() == "route: writing to routing socket: not in table" {
        return if device.is_none() { Some(None) } else { None };
    }
    if !success { return None; }
    device.map(Some)
}

/// The Wi-Fi device, whether it is on and the network it joined: quick reads
/// (`networksetup`, `ipconfig getsummary`). None where there is no Wi-Fi.
pub fn sample_wifi() -> Option<Wifi> {
    #[cfg(target_os = "macos")]
    {
        let device = wifi_device(&run("networksetup", &["-listallhardwareports"])?)?;
        let on = run("networksetup", &["-getairportpower", &device])?.trim_end().ends_with(": On");
        let (joined, secure) = if on { run("ipconfig", &["getsummary", &device]).map_or((false, false), |summary| wifi_link(&summary)) } else { (false, false) };
        Some(Wifi { device, on, joined, secure })
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// The Wi-Fi port's device in `networksetup -listallhardwareports`.
#[cfg(any(target_os = "macos", test))]
fn wifi_device(ports: &str) -> Option<String> {
    let mut lines = ports.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if matches!(line.strip_prefix("Hardware Port:").map(str::trim), Some("Wi-Fi" | "AirPort")) {
            return lines.next()?.strip_prefix("Device:").map(str::trim).filter(|device| !device.is_empty()).map(str::to_string);
        }
    }
    None
}

/// Whether `ipconfig getsummary`'s Wi-Fi joined a network, and whether that
/// one has a lock. Only the summary's own fields count, not its addresses'.
#[cfg(any(target_os = "macos", test))]
fn wifi_link(summary: &str) -> (bool, bool) {
    let field = |key: &str| summary.lines().find_map(|line| line.strip_prefix("  ")?.strip_prefix(key)?.strip_prefix(" : ").map(str::trim));
    let joined = field("LinkStatusActive") == Some("TRUE") && field("SSID").is_some();
    let secure = joined && field("Security").is_some_and(|kind| !kind.is_empty() && !matches!(kind.to_ascii_uppercase().as_str(), "NONE" | "OPEN"));
    (joined, secure)
}

/// Turn the Mac's Wi-Fi on or off, as its menu's switch does. False when
/// `networksetup` refused.
pub fn set_wifi_power(device: &str, on: bool) -> bool {
    run("networksetup", &["-setairportpower", device, if on { "on" } else { "off" }]).is_some()
}

/// Turn the Mac's Bluetooth on or off, as its menu's switch does: IOBluetooth's
/// own preference call (the one `blueutil` makes), looked up when it is
/// needed. False where this macOS has no such call.
pub fn set_bluetooth_power(on: bool) -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        let framework = libc::dlopen(c"/System/Library/Frameworks/IOBluetooth.framework/IOBluetooth".as_ptr(), libc::RTLD_LAZY);
        if framework.is_null() {
            return false;
        }
        let call = libc::dlsym(framework, c"IOBluetoothPreferenceSetControllerPowerState".as_ptr());
        if call.is_null() {
            return false;
        }
        let set: extern "C" fn(i32) = std::mem::transmute(call);
        set(on as i32);
        true
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = on;
        false
    }
}

/// The bluetooth controller's power state — the one bluetooth fact that is
/// cheap to read here (no device list, so the panel says so).
pub fn sample_bluetooth() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        let old = run(
            "defaults",
            &[
                "read",
                "/Library/Preferences/com.apple.Bluetooth",
                "ControllerPowerState",
            ],
        );
        match old.as_deref().map(str::trim) {
            Some("1") => Some(true),
            Some("0") => Some(false),
            _ => run("system_profiler", &["SPBluetoothDataType", "-json"])
                .and_then(|out| parse_bluetooth_profile(&out)),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// System Profiler's machine-readable controller state. Ignore devices and
/// addresses; only an unambiguous known controller state can be shown.
#[cfg(any(target_os = "macos", test))]
fn parse_bluetooth_profile(out: &str) -> Option<bool> {
    let data: serde_json::Value = serde_json::from_str(out).ok()?;
    let entries = data.get("SPBluetoothDataType")?.as_array()?;
    let mut state = None;
    for entry in entries {
        let Some(controller) = entry.get("controller_properties") else { continue };
        let next = match controller.get("controller_state")?.as_str()? {
            "attrib_on" => true,
            "attrib_off" => false,
            _ => return None,
        };
        if state.is_some_and(|previous| previous != next) {
            return None;
        }
        state = Some(next);
    }
    state
}

// ======================================================================
// The surface
// ======================================================================

/// `Bar.qml`: the clusters sit `space(8)` off their edge.
const EDGE_MARGIN: f64 = 8.0;
/// `WidgetButton.horizontalMargin` for the menu button.
const MENU_MARGIN: f64 = 7.5;
/// `WidgetButton.horizontalMargin` for the clock.
const CLOCK_MARGIN: f64 = 8.75;
/// `Workspaces.qml`: `Style.space(20)` per pill, 1px column spacing, and
/// `space(1.5)` of trailing gap after the grid.
const WS_WIDTH: f64 = 20.0;
const WS_SPACING: f64 = 1.0;
const WS_TRAILING: f64 = 1.5;
/// The open-panel pill: 2px thick, inset 2px, `max(10, round(slot*0.55))`.
const PANEL_PILL_THICKNESS: f64 = 2.0;
const PANEL_PILL_INSET: f64 = 2.0;
/// `ActiveWindow.qml`: the title never grows past 280px.
const ACTIVE_WINDOW_MAX: f64 = 280.0;
/// `Ui/Button.qml` tooltip `delay` — 400ms of hover before it shows.
const TOOLTIP_DELAY: f64 = 0.4;
// A Mac's menu bar: 13-point text, 15-point status glyphs, items with room for
// a wash, a translucent strip over the wallpaper.
const MAC_BAR_TEXT: f64 = 13.0;
const MAC_BAR_ICON: f64 = 15.0;
const MAC_BAR_SLOT: f64 = 28.0;
const MAC_BAR_PAD: f64 = 8.0;
const MAC_BAR_ALPHA: f32 = 0.74;
const MAC_BAR_ALPHA_DARK: f32 = 0.62;
/// A Mac's own glyph where it draws a status item differently: its Wi-Fi is arcs
/// over a fan, not the other styles' solid fan.
fn mac_glyph(mac: bool, ico: Ico) -> Ico {
    match (mac, ico) {
        (true, Ico::Wifi) => Ico::WifiArcs,
        (true, Ico::WifiOff) => Ico::WifiArcsOff,
        _ => ico,
    }
}

/// The window controls: three slots of this width at the bar's far right,
/// the usual caption-button proportion (wider than an icon slot, so a
/// close is an easy target), a gap before the status modules.
const WINDOW_CONTROL_WIDTH: f64 = 36.0;
const WINDOW_CONTROL_GAP: f64 = 6.0;

/// The module whose recorded hit rect holds `p` — what the WM's drag query
/// asks (`shell_bar_claims`): a claimed point is a button, not a drag.
pub fn module_at_in(hits: &[(BarModule, Rect)], p: Vec2d) -> Option<BarModule> {
    hits.iter().find(|(_, r)| contains(*r, p)).map(|(m, _)| *m)
}

/// Whether this platform draws its window controls in the bar: every
/// platform but macOS, whose traffic lights sit natively on the left.
pub const fn window_controls_default() -> bool {
    !cfg!(target_os = "macos")
}

/// Where the right side of the bar starts, given the strip's right edge:
/// the x the window controls occupy from (`None` without them) and the x
/// the status modules are laid out leftwards from. Pure, so the layout is
/// tested without a window.
pub fn right_cluster_layout(bar_right: f64, controls: bool) -> (Option<f64>, f64) {
    let edge = bar_right - EDGE_MARGIN;
    if !controls {
        return (None, edge);
    }
    let controls_left = edge - 3.0 * WINDOW_CONTROL_WIDTH;
    (Some(controls_left), controls_left - WINDOW_CONTROL_GAP)
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ShellBarBase = #(ShellBar::register_widget(vm))
    mod.widgets.ShellBar = set_type_default() do mod.widgets.ShellBarBase {
        width: Fill
        height: 26
        draw_bg +: {}
        d +: {}
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum ShellBarAction {
    /// A module was pressed (left button).
    Press(BarModule),
    /// A module was right-pressed — the clock cycles its format, audio
    /// mutes, power toggles the percentage.
    RightPress(BarModule),
    /// A module was middle-pressed (`ActiveWindow.qml`'s
    /// `Qt.MiddleButton` arm: only the active-window title answers to
    /// this, closing the focused client exactly like a right click does).
    MiddlePress(BarModule),
    /// Wheel over a module: +1 / -1 notch.
    Wheel(BarModule, f64),
    #[default]
    None,
}

/// `MouseDown.button` -> the press action it raises. Middle is checked
/// before secondary — a mouse reporting both bits on the same event (some
/// drivers do, for a chorded click) still reads as the tertiary button,
/// matching `ActiveWindow.qml`'s own `Qt.MiddleButton` branch order.
fn press_action(module: BarModule, button: MouseButton) -> ShellBarAction {
    if button.contains(MouseButton::MIDDLE) {
        ShellBarAction::MiddlePress(module)
    } else if button.contains(MouseButton::SECONDARY) {
        ShellBarAction::RightPress(module)
    } else {
        ShellBarAction::Press(module)
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct ShellBar {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    draw_bg: DrawShellFill,
    #[live]
    d: ShellDraw,
    #[live]
    tokens: ShellTokens,
    #[rust]
    pub data: BarData,
    /// Content starts here, so the OS window buttons stay clear.
    #[rust]
    pub pad_left: f64,
    #[rust]
    area: Area,
    /// The rect the bar was last drawn into — the gallery drives these
    /// surfaces directly, without a turtle of their own.
    #[rust]
    screen: Rect,
    #[rust]
    hits: Vec<(BarModule, Rect)>,
    #[rust]
    hover: Option<BarModule>,
    #[rust]
    hover_time: f64,
    #[rust]
    next_frame: NextFrame,
    #[rust]
    last_time: f64,
    /// The indicator block reveals its inactive glyphs while hovered.
    #[rust]
    reveal_indicators: bool,
    #[rust]
    pub inert: bool,
    /// Draw the window's min/max/close at the far right
    /// ([`window_controls_default`]; the gallery turns it on to show them).
    #[rust]
    pub window_controls: bool,
}

impl ShellBar {
    pub fn set_data(&mut self, cx: &mut Cx, data: BarData) {
        self.data = data;
        self.redraw(cx);
    }

    fn icon_slot(&self) -> f64 {
        self.tokens.bar.icon_slot
    }

    fn status_slot(&self) -> f64 {
        self.tokens.bar.status_slot
    }

    /// The status modules on the right, in `shell.json` order, with the
    /// icon each one shows right now.
    fn right_modules(&self) -> Vec<(BarModule, Ico, bool)> {
        let mut v: Vec<(BarModule, Ico, bool)> = Vec::new();
        if self.data.glance.is_some() {
            v.push((BarModule::Glance, Ico::Bell, true));
        }
        for (i, ico) in self.data.tray.iter().enumerate() {
            v.push((BarModule::Tray(i), *ico, true));
        }
        v.push((
            BarModule::Bluetooth,
            match self.data.bluetooth {
                Some(true) => Ico::Bluetooth,
                _ => Ico::BluetoothOff,
            },
            self.data.bluetooth.is_some(),
        ));
        v.push((
            BarModule::Network,
            match self.data.network {
                Some(true) => Ico::Wifi,
                _ => Ico::WifiOff,
            },
            self.data.network.is_some(),
        ));
        v.push((
            BarModule::Audio,
            volume_icon(self.data.volume, self.data.muted),
            self.data.volume.is_some(),
        ));
        v.push((
            BarModule::Monitor,
            Ico::Monitor,
            self.data.brightness.is_some(),
        ));
        v.push((
            BarModule::Power,
            if self.data.battery.is_some() {
                Ico::Battery
            } else {
                Ico::Power
            },
            self.data.battery.is_some(),
        ));
        v
    }

    /// The material the kit paints this bar with; the next draw reads it
    /// (`App::apply_material_to_chrome` redraws everything after the
    /// fan-out).
    pub fn set_material(&mut self, m: MaterialTokens, palette: Option<super::ShellPalette>) {
        self.d.set_material(m);
        self.d.set_palette(palette);
    }

    /// Draw the bar into `r`. Returns nothing; hit rects are recorded for
    /// the next event pass. Under glass the whole bar — strip, modules,
    /// tooltip — is hoisted into the kit's overlay list.
    pub fn draw_bar(&mut self, cx: &mut Cx2d, r: Rect) {
        self.d.begin_surface(cx);
        self.draw_bar_inner(cx, r);
        self.d.end_surface(cx);
    }

    fn draw_bar_inner(&mut self, cx: &mut Cx2d, r: Rect) {
        if self.data.style.mac_family() {
            self.draw_mac_bar(cx, r);
            return;
        }
        let tok = self.d.tokens(self.tokens);
        let fg = tok.bar.text;
        let accent = tok.bar.active;
        // New glance cards wear the palette's accent (the toasts' colour),
        // not the bar's alert one.
        let news = tok.notifications.countdown;
        let slot = self.icon_slot();
        let canvas = tok.bar.icon_canvas;
        self.hits.clear();
        self.screen = r;

        // The strip itself: `Color.bar.background`, no border. Under glass
        // the material paints it and this fill goes transparent — still
        // drawn, as the widget's `#[redraw]` anchor. (`glass_strip` is a
        // no-op under flat.)
        self.draw_bg.color = if self.d.material().is_glass() {
            Vec4f::default()
        } else {
            alpha(tok.bar.background, tok.bar.background_alpha)
        };
        self.draw_bg.draw_abs(cx, r);
        self.d.glass_strip(cx, r);

        // ---- left: menu, workspaces
        let mut x = r.pos.x + self.pad_left.max(EDGE_MARGIN);
        let menu_w = (canvas + MENU_MARGIN * 2.0).max(12.0);
        let menu_rect = rect(x, r.pos.y, menu_w, r.size.y);
        self.d
            .icon_centered(cx, Ico::Menu, menu_rect, canvas, fg);
        self.hits.push((BarModule::Menu, menu_rect));
        x += menu_w;

        for (i, ws) in self.data.workspaces.clone().iter().enumerate() {
            let cell = rect(x, r.pos.y, WS_WIDTH, r.size.y);
            let lit = ws.occupied || ws.focused;
            let color = fade(fg, if lit { 1.0 } else { 0.5 });
            if ws.focused {
                // The focused workspace is a dot, not its number.
                self.d
                    .icon_centered(cx, Ico::Dot, cell, tok.bar.icon_font * 0.5, color);
            } else {
                self.d.label(
                    cx,
                    cell,
                    false,
                    tok.bar.icon_font,
                    color,
                    super::ui::HAlign::Center,
                    &ws.label,
                );
            }
            self.hits.push((BarModule::Workspace(i), cell));
            x += WS_WIDTH + WS_SPACING;
        }
        x += WS_TRAILING;
        let style_label = format!("{}  ▾", self.data.style.label());
        let style_width = self.d.measure(cx, false, tok.font.body, &style_label) + 18.0;
        let style_rect = rect(x, r.pos.y, style_width, r.size.y);
        self.d.label(cx, style_rect, false, tok.font.body, fg, super::ui::HAlign::Center, &style_label);
        self.hits.push((BarModule::Style, style_rect));
        x += style_width + 6.0;

        if self.data.style.supports_dark() {
            let label=if self.data.dark {"Dark"}else{"Light"};
            let width=self.d.measure(cx,false,tok.font.body,label)+18.0;
            let button=rect(x,r.pos.y,width,r.size.y);
            self.d.label(cx,button,false,tok.font.body,fg,super::ui::HAlign::Center,label);
            self.hits.push((BarModule::Appearance,button));
            x+=width+6.0;
        }
        // The active window's title: `min(280, implicitWidth) + controlPaddingX*2`,
        // `body` at α .85, elided right, with the full title in the tooltip.
        if let Some(title) = self.data.active_window.clone() {
            let text_w = self
                .d
                .measure(cx, false, tok.font.body, &title)
                .min(ACTIVE_WINDOW_MAX);
            let w = text_w + tok.spacing.control_padding_x * 2.0;
            let cell = rect(x, r.pos.y, w, r.size.y);
            self.d.label_elided(
                cx,
                rect(
                    cell.pos.x + tok.spacing.control_padding_x,
                    cell.pos.y,
                    text_w,
                    cell.size.y,
                ),
                false,
                tok.font.body,
                fade(fg, 0.85),
                super::ui::HAlign::Left,
                &title,
            );
            self.hits.push((BarModule::ActiveWindow, cell));
            x += w;
        }
        // "Ask <app>": the focused app's agent, in the shell's panel.
        if let Some(name) = self.data.ask_agent.clone() {
            let label = format!("Ask {name}");
            let w = self.d.measure(cx, false, tok.font.body, &label) + tok.spacing.control_padding_x * 2.0;
            let cell = rect(x, r.pos.y, w, r.size.y);
            self.d.label(cx, cell, false, tok.font.body, accent, super::ui::HAlign::Center, &label);
            self.hits.push((BarModule::AskAgent, cell));
        }

        // ---- center: the clock is the anchor, centered on the bar itself
        let clock_w = self
            .d
            .measure(cx, false, tok.font.body, &self.data.clock)
            + CLOCK_MARGIN * 2.0;
        let clock_rect = rect(
            (r.pos.x + (r.size.x - clock_w) * 0.5).floor(),
            r.pos.y,
            clock_w,
            r.size.y,
        );
        self.d.label(
            cx,
            clock_rect,
            false,
            tok.font.body,
            fg,
            super::ui::HAlign::Center,
            &self.data.clock,
        );
        self.hits.push((BarModule::Clock, clock_rect));

        // Indicators sit flush against the anchor's left edge.
        let status = self.status_slot();
        let indicators = self.data.indicators.clone();
        let mut ix = clock_rect.pos.x;
        for (i, ind) in indicators.iter().enumerate().rev() {
            let visible = ind.active || self.reveal_indicators;
            if !visible {
                continue;
            }
            ix -= status;
            let cell = rect(ix, r.pos.y, status, r.size.y);
            let color = if ind.active {
                accent
            } else {
                fade(fg, 0.45)
            };
            let ico = if ind.active { ind.active_icon } else { ind.icon };
            self.d
                .icon_centered(cx, ico, cell, tok.font.caption * 1.3, color);
            self.hits.push((BarModule::Indicator(i), cell));
        }

        // Keyboard layout, weather and the update dot follow the anchor.
        let mut cx_right = clock_rect.pos.x + clock_rect.size.x;
        if let Some(layout) = self.data.keyboard_layout.clone() {
            let w = self.d.measure(cx, false, tok.font.caption, &layout) + 6.0 * 2.0;
            let cell = rect(cx_right, r.pos.y, w, r.size.y);
            self.d.label(
                cx,
                cell,
                false,
                tok.font.caption,
                fg,
                super::ui::HAlign::Center,
                &layout,
            );
            self.hits.push((BarModule::KeyboardLayout, cell));
            cx_right += w;
        }
        if let Some(weather) = self.data.weather.clone() {
            let w = self.d.measure(cx, false, tok.font.caption, &weather) + 6.0 * 2.0;
            let cell = rect(cx_right, r.pos.y, w, r.size.y);
            self.d.label(
                cx,
                cell,
                false,
                tok.font.caption,
                fg,
                super::ui::HAlign::Center,
                &weather,
            );
            self.hits.push((BarModule::Weather, cell));
            cx_right += w;
        }
        if self.data.system_update {
            let cell = rect(cx_right, r.pos.y, status, r.size.y);
            self.d
                .icon_centered(cx, Ico::Refresh, cell, tok.font.caption * 1.3, accent);
            self.hits.push((BarModule::SystemUpdate, cell));
        }

        // ---- right: the window controls at the very edge (where the
        // platform draws none), then tray and the status modules, laid out
        // right to left
        let (controls_left, modules_right) =
            right_cluster_layout(r.pos.x + r.size.x, self.window_controls);
        if let Some(mut cx_ctrl) = controls_left {
            let controls = [
                (BarModule::WindowMin, Ico::WindowMin),
                (
                    BarModule::WindowMax,
                    if self.data.maximized { Ico::WindowRestore } else { Ico::WindowMax },
                ),
                (BarModule::WindowClose, Ico::Close),
            ];
            for (module, ico) in controls {
                let cell = rect(cx_ctrl, r.pos.y, WINDOW_CONTROL_WIDTH, r.size.y);
                if self.hover == Some(module) {
                    // The hovered control lights its slot, a close in the
                    // accent, like every desktop's caption buttons.
                    let wash = if module == BarModule::WindowClose { accent } else { fg };
                    self.d.solid(cx, cell, fade(wash, 0.18));
                }
                self.d.icon_centered(cx, ico, cell, canvas * 0.8, fg);
                self.hits.push((module, cell));
                cx_ctrl += WINDOW_CONTROL_WIDTH;
            }
        }
        let modules = self.right_modules();
        let mut rx = modules_right;
        for (module, ico, available) in modules.iter().rev() {
            rx -= slot;
            let cell = rect(rx, r.pos.y, slot, r.size.y);
            let mut color = if *available { fg } else { fade(fg, 0.45) };
            // The battery goes urgent below 20% on battery power.
            if *module == BarModule::Power {
                if let Some(b) = self.data.battery {
                    if !b.charging && b.percent <= 20 {
                        color = accent;
                    }
                }
            }
            if *module == BarModule::Audio && self.data.muted {
                color = fade(fg, 0.45);
            }
            let glance = if *module == BarModule::Glance { self.data.glance } else { None };
            if let Some((_, new)) = glance {
                if new > 0 {
                    color = news;
                }
            }
            self.d.icon_centered(cx, *ico, cell, canvas, color);
            // The glance button's count, at the bell's corner: a tag in the
            // palette's accent while some cards are new.
            if let Some((count, new)) = glance.filter(|(count, _)| *count > 0) {
                let text = if count > 9 { "9+".to_string() } else { count.to_string() };
                let px = tok.font.caption * 0.8;
                let h = (px * 1.25).round();
                let w = (self.d.measure(cx, true, px, &text) + 4.0).max(h);
                let badge = rect((cell.pos.x + cell.size.x * 0.5 + canvas * 0.15).round(), (cell.pos.y + cell.size.y * 0.5 - canvas * 0.6).round(), w, h);
                if new > 0 {
                    self.d.solid(cx, badge, news);
                    self.d.label(cx, badge, true, px, tok.bar.background, super::ui::HAlign::Center, &text);
                } else {
                    self.d.label(cx, badge, true, px, fade(fg, 0.7), super::ui::HAlign::Center, &text);
                }
            }
            self.hits.push((*module, cell));
        }

        // The open-panel pill, at the bar's inner (bottom) edge.
        if let Some(open) = self.data.open_panel {
            if let Some((_, cell)) = self.hits.iter().find(|(m, _)| *m == open) {
                let extent = (slot * 0.55).round().max(10.0);
                let pill = rect(
                    (cell.pos.x + (cell.size.x - extent) * 0.5).floor(),
                    r.pos.y + r.size.y - PANEL_PILL_INSET - PANEL_PILL_THICKNESS,
                    extent,
                    PANEL_PILL_THICKNESS,
                );
                self.d.solid(cx, pill, fade(accent, 0.9));
            }
        }

        // The hover tooltip, once the pointer has rested 400ms.
        if let Some(module) = self.hover {
            if self.hover_time >= TOOLTIP_DELAY {
                self.draw_tooltip(cx, r, module);
            }
        }
    }

    /// A Mac's menu bar: the system mark, the focused app's name in bold and
    /// what the app offers; the status items and the clock at the right edge.
    /// The modules are the same as on every other bar, so their presses,
    /// panels and tooltips work unchanged; only their places differ.
    fn draw_mac_bar(&mut self, cx: &mut Cx2d, r: Rect) {
        let tok = self.d.tokens(self.tokens);
        let fg = tok.bar.text;
        let news = tok.notifications.countdown;
        let slot = MAC_BAR_SLOT;
        let canvas = MAC_BAR_ICON;
        let text = MAC_BAR_TEXT;
        self.hits.clear();
        self.screen = r;
        // Launchpad takes the whole screen, as on a Mac.
        if self.data.style == crate::desktop::DesktopStyle::Macos && crate::desktop::mac_launchpad() {
            self.draw_bg.color = Vec4f::default();
            self.draw_bg.draw_abs(cx, r);
            return;
        }
        self.draw_bg.color = if self.d.material().is_glass() {
            Vec4f::default()
        } else {
            let opacity = if self.data.dark { MAC_BAR_ALPHA_DARK } else { MAC_BAR_ALPHA };
            alpha(tok.bar.background, opacity)
        };
        self.draw_bg.draw_abs(cx, r);
        self.d.glass_strip_plain(cx, r);

        let mac = self.data.style == crate::desktop::DesktopStyle::Macos;
        // ---- left: the mark, the workspaces when there are several (a Mac's are a
        // status item: nothing sits between its mark and its menus), the app
        let mut x = r.pos.x + self.pad_left.max(EDGE_MARGIN);
        let mark = rect(x, r.pos.y, slot + 4.0, r.size.y);
        self.mac_wash(cx, BarModule::Menu, mark, fg);
        self.d.icon_centered(cx, Ico::Octopus, mark, canvas, fg);
        self.hits.push((BarModule::Menu, mark));
        x += mark.size.x + 4.0;
        if !mac && self.data.workspaces.len() > 1 {
            for (i, ws) in self.data.workspaces.clone().iter().enumerate() {
                let cell = rect(x, r.pos.y, WS_WIDTH, r.size.y);
                self.mac_wash(cx, BarModule::Workspace(i), cell, fg);
                let color = fade(fg, if ws.focused { 1.0 } else { 0.55 });
                self.d.label(cx, cell, ws.focused, text - 1.0, color, super::ui::HAlign::Center, &ws.label);
                self.hits.push((BarModule::Workspace(i), cell));
                x += WS_WIDTH + WS_SPACING;
            }
            x += 6.0;
        }
        // A Mac always names the app in front; with no window up, the desktop's.
        let active = self.data.active_app.clone().or_else(|| mac.then(|| "OctoSense".to_string()));
        if let Some(name) = active {
            let name_w = self.d.measure(cx, true, text, &name).min(ACTIVE_WINDOW_MAX);
            let cell = rect(x, r.pos.y, name_w + MAC_BAR_PAD * 2.0, r.size.y);
            self.mac_wash(cx, BarModule::ActiveWindow, cell, fg);
            self.d.label_elided(cx, rect(cell.pos.x + MAC_BAR_PAD, cell.pos.y, name_w, cell.size.y), true, text, fg, super::ui::HAlign::Left, &name);
            self.hits.push((BarModule::ActiveWindow, cell));
            x += cell.size.x;
        }
        let (controls_left, modules_right) = right_cluster_layout(r.pos.x + r.size.x, self.window_controls);
        let mut clock = if self.data.mac_clock.is_empty() { self.data.clock.clone() } else { self.data.mac_clock.clone() };
        if mac {
            // On a narrow screen the clock gives up its date before a menu goes, and then
            // the last menu goes first, as on a Mac. What's on the right never gives way.
            let time_box = self.data.time_box.map_or(0.0, |left| self.d.measure(cx, false, text, &format!("{left} min")) + 8.0 + 5.0 + MAC_BAR_PAD * 2.0);
            let titles: f64 = self.data.mac_titles.clone().iter().map(|title| self.d.measure(cx, false, text, title) + MAC_BAR_PAD * 2.0).sum();
            let own = slot * 2.0;
            let room = modules_right - time_box - own - x;
            if titles + self.d.measure(cx, false, text, &clock) + MAC_BAR_PAD * 2.0 > room {
                if let Some((_, time)) = clock.rsplit_once("  ") {
                    clock = time.to_string();
                }
            }
            let titles_end = x + room - self.d.measure(cx, false, text, &clock) - MAC_BAR_PAD * 2.0;
            for (i, title) in self.data.mac_titles.clone().iter().enumerate() {
                let w = self.d.measure(cx, false, text, title) + MAC_BAR_PAD * 2.0;
                if x + w > titles_end {
                    break;
                }
                let cell = rect(x, r.pos.y, w, r.size.y);
                self.mac_wash(cx, BarModule::MacMenu(i), cell, fg);
                self.d.label(cx, cell, false, text, fg, super::ui::HAlign::Center, title);
                self.hits.push((BarModule::MacMenu(i), cell));
                x += w;
            }
        } else if let Some(name) = self.data.ask_agent.clone() {
            let label = format!("Ask {name}");
            let w = self.d.measure(cx, false, text, &label) + MAC_BAR_PAD * 2.0;
            let cell = rect(x, r.pos.y, w, r.size.y);
            self.mac_wash(cx, BarModule::AskAgent, cell, fg);
            self.d.label(cx, cell, false, text, fg, super::ui::HAlign::Center, &label);
            self.hits.push((BarModule::AskAgent, cell));
        }

        // ---- right: the window's controls where the platform draws none,
        // then the clock at the edge and the status items before it
        if let Some(mut cx_ctrl) = controls_left {
            let controls = [
                (BarModule::WindowMin, Ico::WindowMin),
                (BarModule::WindowMax, if self.data.maximized { Ico::WindowRestore } else { Ico::WindowMax }),
                (BarModule::WindowClose, Ico::Close),
            ];
            for (module, ico) in controls {
                let cell = rect(cx_ctrl, r.pos.y, WINDOW_CONTROL_WIDTH, r.size.y);
                self.mac_wash(cx, module, cell, fg);
                self.d.icon_centered(cx, ico, cell, canvas * 0.8, fg);
                self.hits.push((module, cell));
                cx_ctrl += WINDOW_CONTROL_WIDTH;
            }
        }
        let clock_w = self.d.measure(cx, false, text, &clock) + MAC_BAR_PAD * 2.0;
        let clock_rect = rect((modules_right - clock_w).floor(), r.pos.y, clock_w, r.size.y);
        self.mac_wash(cx, BarModule::Clock, clock_rect, fg);
        self.d.label(cx, clock_rect, false, text, fg, super::ui::HAlign::Center, &clock);
        self.hits.push((BarModule::Clock, clock_rect));
        let mut rx = clock_rect.pos.x;
        // A Mac's own items, which never give way: Control Center by the clock, then Spotlight.
        if mac {
            for (module, ico) in [(BarModule::ControlCenter, Ico::Controls), (BarModule::Spotlight, Ico::Search)] {
                rx -= slot;
                let cell = rect(rx, r.pos.y, slot, r.size.y);
                self.mac_wash(cx, module, cell, fg);
                self.d.icon_centered(cx, ico, cell, canvas, fg);
                self.hits.push((module, cell));
            }
        }
        // A running time box is never one that gives way below: this must not go unsaid.
        if let Some(left) = self.data.time_box.filter(|_| mac) {
            let label = format!("{left} min");
            let label_w = self.d.measure(cx, false, text, &label);
            let w = label_w + 8.0 + 5.0 + MAC_BAR_PAD * 2.0;
            rx -= w;
            let cell = rect(rx, r.pos.y, w, r.size.y);
            self.mac_wash(cx, BarModule::TimeBox, cell, fg);
            let amber = crate::approvals::mac_kit::mac_ink(self.data.dark).amber;
            self.d.rounded(cx, rect(cell.pos.x + MAC_BAR_PAD, (cell.pos.y + (cell.size.y - 8.0) * 0.5).round(), 8.0, 8.0), 2.0, amber);
            self.d.label(cx, rect(cell.pos.x + MAC_BAR_PAD + 13.0, cell.pos.y, label_w, cell.size.y), false, text, fg, super::ui::HAlign::Left, &label);
            self.hits.push((BarModule::TimeBox, cell));
        }
        // On a narrow screen a Mac's status items give way to its menus.
        let no_room = |rx: f64, w: f64| mac && rx - w < x;
        let modules = self.right_modules();
        for (module, ico, available) in modules.iter().rev() {
            // A Mac shows no item for what this machine doesn't have: no battery, no display
            // control. Its clock opens the notifications, so it has no bell either.
            if mac && (!*available || *module == BarModule::Glance) {
                continue;
            }
            // A Mac's battery is the widest of them: its charge shows in it.
            let battery = self.data.battery.filter(|_| mac && *module == BarModule::Power);
            let slot = slot + if battery.is_some() { 6.0 } else { 0.0 };
            if no_room(rx, slot) {
                break;
            }
            rx -= slot;
            let cell = rect(rx, r.pos.y, slot, r.size.y);
            let mut color = if *available { fg } else { fade(fg, 0.4) };
            if *module == BarModule::Power {
                if let Some(b) = self.data.battery {
                    if !b.charging && b.percent <= 20 {
                        color = tok.bar.active;
                    }
                }
            }
            if *module == BarModule::Audio && self.data.muted {
                color = fade(fg, 0.4);
            }
            let glance = if *module == BarModule::Glance { self.data.glance } else { None };
            if glance.is_some_and(|(_, new)| new > 0) {
                color = news;
            }
            self.mac_wash(cx, *module, cell, fg);
            match battery {
                Some(b) => self.draw_mac_battery(cx, cell, b, fg),
                None => self.d.icon_centered(cx, mac_glyph(mac, *ico), cell, canvas, color),
            }
            if let Some((count, new)) = glance.filter(|(count, _)| *count > 0) {
                let label = if count > 9 { "9+".to_string() } else { count.to_string() };
                let px = tok.font.caption * 0.8;
                let h = (px * 1.25).round();
                let w = (self.d.measure(cx, true, px, &label) + 4.0).max(h);
                let badge = rect((cell.pos.x + cell.size.x * 0.5 + canvas * 0.15).round(), (cell.pos.y + cell.size.y * 0.5 - canvas * 0.6).round(), w, h);
                if new > 0 {
                    self.d.solid(cx, badge, news);
                    self.d.label(cx, badge, true, px, tok.bar.background, super::ui::HAlign::Center, &label);
                } else {
                    self.d.label(cx, badge, true, px, fade(fg, 0.7), super::ui::HAlign::Center, &label);
                }
            }
            self.hits.push((*module, cell));
        }
        // A Mac's menu bar has no item for its desktops: the keys reach them
        // (⌃1 to ⌃9, ⌃← and ⌃→, listed under Keyboard Shortcuts), and the
        // Dock goes to the desktop of the window it brings forward.
        // The appearance; the style lives under the system mark's menu. A Mac keeps
        // both there.
        if !mac && self.data.style.supports_dark() && !no_room(rx, slot) {
            rx -= slot;
            let cell = rect(rx, r.pos.y, slot, r.size.y);
            self.mac_wash(cx, BarModule::Appearance, cell, fg);
            self.d.icon_centered(cx, if self.data.dark { Ico::Moon } else { Ico::Brightness }, cell, canvas, fg);
            self.hits.push((BarModule::Appearance, cell));
        }
        // On a Mac the assistant is a status item, and a status item stays put whatever
        // app is in front: the front app's agent when it has one, else OctoSense's own.
        if mac && !no_room(rx, slot) {
            rx -= slot;
            let cell = rect(rx, r.pos.y, slot, r.size.y);
            self.mac_wash(cx, BarModule::AskAgent, cell, fg);
            self.d.icon_centered(cx, Ico::Sparkle, cell, canvas, fg);
            self.hits.push((BarModule::AskAgent, cell));
        }
        // What the system is doing (recording, say): only while it does.
        let status = self.status_slot();
        for (i, ind) in self.data.indicators.clone().iter().enumerate().rev() {
            if !ind.active {
                continue;
            }
            rx -= status;
            let cell = rect(rx, r.pos.y, status, r.size.y);
            self.d.icon_centered(cx, ind.active_icon, cell, canvas, tok.bar.active);
            self.hits.push((BarModule::Indicator(i), cell));
        }
        if self.data.system_update {
            rx -= status;
            let cell = rect(rx, r.pos.y, status, r.size.y);
            self.d.icon_centered(cx, Ico::Refresh, cell, canvas, tok.bar.active);
            self.hits.push((BarModule::SystemUpdate, cell));
        }

        if let Some(module) = self.hover {
            if self.hover_time >= TOOLTIP_DELAY {
                self.draw_tooltip(cx, r, module);
            }
        }
    }

    /// A menu bar item's rounded wash, under the pointer or while its panel
    /// is open.
    /// A Mac's battery item: the charge in its body, a bolt on the adapter, red when it runs low.
    fn draw_mac_battery(&mut self, cx: &mut Cx2d, cell: Rect, b: Battery, fg: Vec4f) {
        let (w, h) = (22.0, 11.0);
        let x = (cell.pos.x + (cell.size.x - w - 2.5) * 0.5).round();
        let y = (cell.pos.y + (cell.size.y - h) * 0.5).round();
        self.d.focus_ring(cx, rect(x, y, w, h), 3.0, 1.0, fade(fg, 0.5));
        self.d.rounded(cx, rect(x + w + 1.0, y + 3.5, 1.5, h - 7.0), 0.75, fade(fg, 0.5));
        let charge = (w - 4.0) * (b.percent.min(100) as f64 / 100.0);
        let ink = if b.charging {
            fade(fg, 0.45)
        } else if b.percent <= 20 {
            crate::approvals::mac_kit::mac_ink(self.data.dark).danger
        } else {
            fg
        };
        self.d.rounded(cx, rect(x + 2.0, y + 2.0, charge.max(2.0), h - 4.0), 1.5, ink);
        if b.charging {
            self.d.icon_centered(cx, Ico::Bolt, rect(x, y, w, h), 10.0, fg);
        }
    }

    fn mac_wash(&mut self, cx: &mut Cx2d, module: BarModule, cell: Rect, fg: Vec4f) {
        let open = self.data.open_panel == Some(module);
        let mac = self.data.style == crate::desktop::DesktopStyle::Macos;
        // A Mac's menu bar lights an item only while its menu is open.
        if !open && (mac || self.hover != Some(module)) {
            return;
        }
        let wash = rect(cell.pos.x, cell.pos.y + 2.0, cell.size.x, cell.size.y - 4.0);
        self.d.rounded(cx, wash, 2.5, fade(fg, if open { 0.2 } else { 0.1 }));
    }

    /// The tooltip a module shows after 400ms of hover.
    fn tooltip_for(&self, module: BarModule) -> String {
        match module {
            BarModule::Style => "Choose operating system style".into(),
            BarModule::Appearance => "Toggle light / dark appearance".into(),
            BarModule::Menu => "Applications".into(),
            BarModule::Workspace(i) => format!("Workspace {}", i + 1),
            BarModule::Desktops => "Desktops".into(),
            BarModule::ActiveWindow => self
                .data
                .active_window
                .clone()
                .unwrap_or_default(),
            BarModule::AskAgent => match self.data.ask_agent.clone() {
                Some(name) => format!("Talk to {name}'s agent (Shift+F8)"),
                None => "Ask OctoSense (F8)".into(),
            },
            BarModule::Glance => match self.data.glance {
                Some((count, new)) if new > 0 => format!("At a glance (F9): {count} card(s), {new} new"),
                Some((count, _)) => format!("At a glance (F9): {count} card(s)"),
                None => "At a glance (F9)".into(),
            },
            BarModule::Clock if self.data.style == crate::desktop::DesktopStyle::Macos => "Notifications".into(),
            BarModule::Clock => "Calendar".into(),
            BarModule::KeyboardLayout => "Keyboard layout".into(),
            BarModule::Weather => "Weather".into(),
            BarModule::SystemUpdate => "Pending updates".into(),
            BarModule::TimeBox => format!("Approving everything an app asks, {} min left", self.data.time_box.unwrap_or(0)),
            BarModule::Tray(_) => "Tray".into(),
            BarModule::Bluetooth => match self.data.bluetooth {
                Some(true) => "Bluetooth on".into(),
                Some(false) => "Bluetooth off".into(),
                None => "Bluetooth unavailable".into(),
            },
            BarModule::Network => match self.data.network {
                Some(true) => "Connected".into(),
                Some(false) => "Not connected".into(),
                None => "Network unavailable".into(),
            },
            BarModule::Audio => match (self.data.volume, self.data.muted) {
                (_, true) => "Muted".into(),
                (Some(v), _) => format!("Volume {}%", v),
                (None, _) => "Audio unavailable".into(),
            },
            BarModule::Monitor => "Display".into(),
            BarModule::Power => match self.data.battery {
                Some(b) if b.charging => format!("Battery {}%, charging", b.percent),
                Some(b) => format!("Battery {}%", b.percent),
                None => "Power".into(),
            },
            BarModule::Indicator(i) => self
                .data
                .indicators
                .get(i)
                .map(|ind| ind.tooltip.to_string())
                .unwrap_or_default(),
            BarModule::WindowMin => "Minimize".into(),
            BarModule::WindowMax => {
                if self.data.maximized { "Restore".into() } else { "Maximize".into() }
            }
            BarModule::WindowClose => "Close".into(),
            BarModule::MacMenu(_) => String::new(),
            BarModule::Spotlight => "Spotlight".into(),
            BarModule::ControlCenter => "Control Center".into(),
        }
    }

    /// `Ui/PanelToolTip.qml`: the tooltip card, `bodySmall` inside
    /// `controlPaddingX/Y`, on `[tooltip] background` behind its 1px
    /// border, 6px off the bar edge.
    fn draw_tooltip(&mut self, cx: &mut Cx2d, r: Rect, module: BarModule) {
        let tok = self.d.tokens(self.tokens);
        let text = self.tooltip_for(module);
        if text.is_empty() {
            return;
        }
        let Some((_, cell)) = self.hits.iter().find(|(m, _)| *m == module).copied() else {
            return;
        };
        let px = tok.font.body_small;
        let tw = self.d.measure(cx, false, px, &text);
        let w = tw + tok.spacing.control_padding_x * 2.0;
        let h = px * 1.4 + tok.spacing.control_padding_y * 2.0;
        let x = (cell.pos.x + cell.size.x * 0.5 - w * 0.5)
            .max(r.pos.x + 2.0)
            .min(r.pos.x + r.size.x - w - 2.0)
            .floor();
        let y = (r.pos.y + r.size.y + 6.0).floor();
        let card = rect(x, y, w, h);
        self.d.card(cx, card, &tok.tooltip);
        self.d.label(
            cx,
            card,
            false,
            px,
            tok.tooltip.text,
            super::ui::HAlign::Center,
            &text,
        );
    }

    /// The Mac's menu titles in the bar's order: the mark, the app, then File and
    /// the rest. Their rects are where the menus open.
    pub fn mac_title_rects(&self) -> Vec<Rect> {
        let find = |module: BarModule| self.module_rect(module).unwrap_or_default();
        let mut rects = vec![find(BarModule::Menu), find(BarModule::ActiveWindow)];
        for i in 0..self.data.mac_titles.len() {
            // A title with no room on a narrow screen: the arrow keys still reach its menu,
            // which opens where the titles end.
            let end = rects.last().map_or(Rect::default(), |last| Rect { pos: dvec2(last.pos.x + last.size.x, last.pos.y), size: dvec2(0.0, last.size.y) });
            rects.push(self.module_rect(BarModule::MacMenu(i)).unwrap_or(end));
        }
        rects
    }

    pub fn module_at(&self, p: Vec2d) -> Option<BarModule> {
        module_at_in(&self.hits, p)
    }

    /// Every item the bar drew, with where it drew it.
    pub fn cells(&self) -> &[(BarModule, Rect)] {
        &self.hits
    }

    /// The rect a module occupies — the panels anchor to it.
    pub fn module_rect(&self, module: BarModule) -> Option<Rect> {
        self.hits
            .iter()
            .find(|(m, _)| *m == module)
            .map(|(_, r)| *r)
    }
}

impl Widget for ShellBar {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let r = cx.turtle().rect();
        self.draw_bar(cx, r);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if self.inert {
            return;
        }
        let bar_rect = self.screen;
        if let Some(ne) = self.next_frame.is_event(event) {
            if self.hover.is_some() {
                let dt = if self.last_time <= 0.0 {
                    1.0 / 60.0
                } else {
                    (ne.time - self.last_time).clamp(0.001, 0.2)
                };
                self.last_time = ne.time;
                let before = self.hover_time;
                self.hover_time += dt;
                if before < TOOLTIP_DELAY && self.hover_time >= TOOLTIP_DELAY {
                    self.redraw(cx);
                }
                if self.hover_time < TOOLTIP_DELAY {
                    self.next_frame = cx.new_next_frame();
                }
            }
        }
        match event {
            Event::MouseMove(e) => {
                let over = contains(bar_rect, e.abs);
                let module = if over { self.module_at(e.abs) } else { None };
                let reveal = matches!(module, Some(BarModule::Indicator(_)))
                    || (over
                        && self
                            .hits
                            .iter()
                            .any(|(m, r)| matches!(m, BarModule::Indicator(_)) && contains(*r, e.abs)));
                if module != self.hover || reveal != self.reveal_indicators {
                    self.hover = module;
                    self.hover_time = 0.0;
                    self.last_time = 0.0;
                    if module.is_some() {
                        self.next_frame = cx.new_next_frame();
                    }
                    self.reveal_indicators = reveal;
                    if over {
                        let cursor = desktop_style::control_cursor(cx);
                        cx.set_cursor(cursor);
                    }
                    self.redraw(cx);
                }
            }
            Event::MouseDown(e) => {
                if let Some(module) = self.module_at(e.abs) {
                    cx.widget_action(self.uid, press_action(module, e.button));
                }
            }
            Event::Scroll(e) => {
                if contains(bar_rect, e.abs) && e.scroll.y.abs() > 0.5 {
                    if let Some(module) = self.module_at(e.abs) {
                        cx.widget_action(
                            self.uid,
                            ShellBarAction::Wheel(module, -e.scroll.y.signum()),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_default_route_is_offline_but_other_lookup_failures_are_unknown() {
        let missing = "route: writing to routing socket: not in table\n";
        assert_eq!(default_route_device("", missing, true), Some(None));
        assert_eq!(default_route_device("", missing, false), Some(None));
        assert_eq!(default_route_device("   interface: en0\n", "", true), Some(Some("en0")));
        assert_eq!(default_route_device("   interface: en0\n", missing, true), None);
        assert_eq!(default_route_device("", "permission denied\n", false), None);
        assert_eq!(default_route_device("", "", true), None);
        assert_eq!(default_route_device("   interface:  \n", "", true), None);
    }

    #[test]
    fn wifi_is_read_from_its_port_and_its_summary_alone() {
        let ports = "Hardware Port: Ethernet\nDevice: en0\nEthernet Address: x\n\nHardware Port: Wi-Fi\nDevice: en1\nEthernet Address: y\n";
        assert_eq!(wifi_device(ports).as_deref(), Some("en1"));
        assert_eq!(wifi_device("Hardware Port: Ethernet\nDevice: en0\n"), None);
        let summary = "<dictionary> {\n  BSSID : <redacted>\n  IPv4 : <array> {\n    0 : <dictionary> {\n      Security : NONE\n    }\n  }\n  InterfaceType : WiFi\n  LinkStatusActive : TRUE\n  SSID : <redacted>\n  Security : WPA2_PSK\n}\n";
        assert_eq!(wifi_link(summary), (true, true));
        assert_eq!(wifi_link(&summary.replace("WPA2_PSK", "NONE")), (true, false), "an open network has no lock");
        assert_eq!(wifi_link(&summary.replace("LinkStatusActive : TRUE", "LinkStatusActive : FALSE")), (false, false));
        assert_eq!(wifi_link("<dictionary> {\n  InterfaceType : WiFi\n  LinkStatusActive : TRUE\n}\n"), (false, false), "no network joined");
    }

    #[test]
    fn bluetooth_profile_accepts_only_a_known_unambiguous_controller_state() {
        assert_eq!(parse_bluetooth_profile(r#"{"SPBluetoothDataType":[{"controller_properties":{"controller_state":"attrib_on"}}]}"#), Some(true));
        assert_eq!(parse_bluetooth_profile(r#"{"SPBluetoothDataType":[{"controller_properties":{"controller_state":"attrib_off"}}]}"#), Some(false));
        assert_eq!(parse_bluetooth_profile(r#"{"SPBluetoothDataType":[{"controller_properties":{"controller_state":"attrib_unknown"}}]}"#), None);
        assert_eq!(parse_bluetooth_profile(r#"{"SPBluetoothDataType":[{"controller_properties":{}}]}"#), None);
        assert_eq!(parse_bluetooth_profile(r#"{"SPBluetoothDataType":[{"controller_properties":{"controller_state":"attrib_on"}},{"controller_properties":{"controller_state":"attrib_off"}}]}"#), None);
        assert_eq!(parse_bluetooth_profile(r#"{"SPBluetoothDataType":[]}"#), None);
        assert_eq!(parse_bluetooth_profile("invalid"), None);
    }

    #[test]
    fn a_macs_battery_is_on_its_adapter_only_when_the_first_line_says_so() {
        let battery = parse_battery("Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t64%; discharging; 3:42 remaining present: true\n").unwrap();
        assert_eq!((battery.percent, battery.charging), (64, false));
        let adapter = parse_battery("Now drawing from 'AC Power'\n -InternalBattery-0 (id=23134307)\t100%; charged; 0:00 remaining present: true\n").unwrap();
        assert_eq!((adapter.percent, adapter.charging), (100, true));
        assert!(parse_battery("Now drawing from 'AC Power'\n").is_none(), "a Mac with no battery");
    }

    #[test]
    fn the_volume_ladder_matches_the_osd_thresholds() {
        assert_eq!(volume_icon(Some(0), false), Ico::Volume0);
        assert_eq!(volume_icon(Some(33), false), Ico::Volume1);
        assert_eq!(volume_icon(Some(34), false), Ico::Volume2);
        assert_eq!(volume_icon(Some(66), false), Ico::Volume2);
        assert_eq!(volume_icon(Some(67), false), Ico::Volume3);
        // Muted always reads as the silent glyph, whatever the level.
        assert_eq!(volume_icon(Some(90), true), Ico::Volume0);
    }

    #[test]
    fn the_window_controls_take_the_far_right_and_push_the_modules_left() {
        // Without controls the status modules end at the edge margin.
        assert_eq!(right_cluster_layout(1000.0, false), (None, 992.0));
        // With them: three slots at the edge, a gap, then the modules.
        let (controls, modules) = right_cluster_layout(1000.0, true);
        assert_eq!(controls, Some(992.0 - 3.0 * WINDOW_CONTROL_WIDTH));
        assert_eq!(modules, 992.0 - 3.0 * WINDOW_CONTROL_WIDTH - WINDOW_CONTROL_GAP);
        assert!(modules < controls.unwrap());
        // The default follows the platform: macOS keeps its own buttons.
        assert_eq!(window_controls_default(), !cfg!(target_os = "macos"));
    }

    /// The drag query asks the bar which points are BUTTONS: the window
    /// controls must claim theirs, or a press on Close would drag the
    /// window instead. `module_at` is that answer; the hit rects the draw
    /// records are what it reads, so this pins the rects the layout gives.
    #[test]
    fn the_window_controls_claim_their_points_from_the_drag_query() {
        // What draw_bar records for the controls, laid out by the same rule.
        let r = rect(0.0, 0.0, 1000.0, 26.0);
        let (controls_left, _) = right_cluster_layout(r.pos.x + r.size.x, true);
        let left = controls_left.unwrap();
        let mut hits: Vec<(BarModule, Rect)> = Vec::new();
        let mut x = left;
        for module in [BarModule::WindowMin, BarModule::WindowMax, BarModule::WindowClose] {
            hits.push((module, rect(x, r.pos.y, WINDOW_CONTROL_WIDTH, r.size.y)));
            x += WINDOW_CONTROL_WIDTH;
        }
        // Inside each control: claimed, by that control. Just left of the
        // first one: nobody's — a drag.
        let mid = r.pos.y + 13.0;
        assert_eq!(module_at_in(&hits, dvec2(left + 18.0, mid)), Some(BarModule::WindowMin));
        assert_eq!(module_at_in(&hits, dvec2(left + 54.0, mid)), Some(BarModule::WindowMax));
        assert_eq!(module_at_in(&hits, dvec2(left + 90.0, mid)), Some(BarModule::WindowClose));
        assert_eq!(module_at_in(&hits, dvec2(left - 2.0, mid)), None);
    }

    #[test]
    fn the_fixture_bar_has_the_shell_json_modules() {
        let data = BarData::fixture();
        assert_eq!(data.workspaces.len(), 5);
        assert!(data.workspaces[0].focused);
        assert_eq!(data.indicators.len(), 3);
        assert!(!data.clock.is_empty());
    }

    /// `ActiveWindow.qml`: left activates, middle and right both close —
    /// but they are DIFFERENT actions, not middle aliased to right, so a
    /// middle click on any other module (clock, audio, …) stays a no-op
    /// instead of firing that module's right-click behavior.
    #[test]
    fn middle_and_right_press_are_distinct_actions() {
        let m = BarModule::ActiveWindow;
        assert_eq!(press_action(m, MouseButton::PRIMARY), ShellBarAction::Press(m));
        assert_eq!(
            press_action(m, MouseButton::SECONDARY),
            ShellBarAction::RightPress(m)
        );
        assert_eq!(
            press_action(m, MouseButton::MIDDLE),
            ShellBarAction::MiddlePress(m)
        );
        assert_ne!(
            press_action(m, MouseButton::MIDDLE),
            press_action(m, MouseButton::SECONDARY)
        );
    }

    /// Script apps' `std.local_time` uses the offset the shell sets, which
    /// is this device's own: the one chrono reads from the same zone data.
    #[test]
    fn script_local_time_follows_the_device_zone() {
        use makepad_widgets::makepad_platform::script::timer::script_local_utc_offset_secs;
        refresh_script_utc_offset();
        let device = chrono::Local::now().offset().local_minus_utc() as i64;
        assert_eq!(script_local_utc_offset_secs(), device);
    }
}
