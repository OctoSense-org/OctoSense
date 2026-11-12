//! Host-owned release UI. No agent tools or contained-app service can install an update.
use makepad_app_module::{
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, ModuleWindows, OpenSchema, ServiceExecutor,
    ValidatedOpen,
};
pub use makepad_widgets;
use makepad_widgets::*;
use octosense_updater::{
    BuildIdentity, Cancellation, Channel, CheckResult, CheckStatus, Client, Offer, Platform,
    Product, VerifiedArtifact,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Mutex, OnceLock,
    },
};

mod install;
#[cfg(target_os = "macos")]
pub mod restart;

static CACHE: OnceLock<PathBuf> = OnceLock::new();
static BUSY: AtomicBool = AtomicBool::new(false);
static REQUEST: AtomicU64 = AtomicU64::new(1);
static ANDROID: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
static PRETEND: OnceLock<String> = OnceLock::new();

/// The shell supplies a host-private directory, outside every app's storage jail.
/// Downloads an earlier run opened or left behind are removed off the UI
/// thread (`octosense_updater::sweep`). On macOS the same thread reads an
/// earlier installation's unseen outcome and removes retained update copies
/// nothing needs (`restart::startup`).
pub fn configure(cache: PathBuf) {
    let stale = cache.clone();
    let _ = std::thread::Builder::new()
        .name("octosense-updater-sweep".into())
        .spawn(move || {
            octosense_updater::sweep(&stale);
            #[cfg(target_os = "macos")]
            restart::startup(&stale);
        });
    let _ = CACHE.set(cache);
}

/// For checking the desktop pane's states on a development build
/// (`--test-action updater-test:<value>`, the local user's own flag): read the
/// build as the release `<value>` names (`desktop-v0.1.0-rc.5`, `+restart` to
/// offer Install & Restart), or as `unsupported`. The release checks, downloads
/// and verification stay the real ones, and Install & Restart still acts only
/// on a real click. A release build's identity is compiled in; this changes
/// nothing there (`test_identity`).
pub fn pretend(value: &str) {
    let _ = PRETEND.set(value.to_owned());
}

/// Whether an installation step acts only on real input, Makepad's input
/// provenance, which a script, a remote control or a synthetic click cannot
/// produce: Android's install review, and macOS Install & Restart, which
/// quits OctoSense and replaces it.
fn requires_real_input(automatic: bool) -> bool {
    cfg!(target_os = "android") || automatic
}

/// Android integration packets are consumed by the shell before module dispatch.
pub fn android_result(payload: &str) {
    if payload.len() > 8192 {
        return;
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
        let mut pending = ANDROID.lock().unwrap_or_else(|e| e.into_inner());
        if pending.len() == 32 {
            pending.remove(0);
        }
        pending.push(value);
        makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
    }
}

script_mod! {
    use mod.prelude.widgets.*

    // A desktop style's sheet says `theme.ui_pointer`: `DeskBody` then draws
    // the Mac's Software Update pane. Without it (a phone) `TouchBody` is the
    // app. The `try` matters: a missing property ends a `script_mod!` evaluation.
    // A shader's border_radius is half the corner it draws: 3 is a 6-point corner.
    let desk = try { theme.ui_pointer == true } catch { false }
    let desk_mac = desk && try { theme.ui_unified == true } catch { false }
    let d_ink = theme.color_text
    let d_page = theme.color_bg_app
    let d_light = dot(d_ink, #x36b61200) < 0.5
    let d_surface = if d_light { #xffffff } else { mix(d_page, d_ink, 0.07) }
    let d_border = mix(d_page, d_ink, 0.14)
    let d_muted = mix(d_ink, d_page, 0.26)
    let d_dim = mix(d_ink, d_page, 0.56)
    let d_selected = mix(d_page, d_ink, 0.11)
    // A progress bar's groove: in dark it stands off the box it sits in, as a Mac's does.
    let d_track = if d_light { d_selected } else { mix(d_surface, d_ink, 0.16) }
    let d_button = if d_light { #xffffff } else { mix(d_page, d_ink, 0.16) }
    let d_primary = #x0071e3
    // A link's blue, the one the other apps' links use (apps/interface.splash `ui_link`): deeper
    // than the button blue on white, lighter on the dark window, where the button blue is too dim to read.
    let d_link = if d_light { #x0060c0 } else { #x6cb8ff }
    let d_clear = #x00000000
    let DeskText = Label{width: Fill padding: 0 clip_y: false text: "" draw_text +: {color: d_ink wrap: Words text_style: theme.font_regular{font_size: 9.75}}}
    let DeskBold = DeskText{draw_text +: {text_style: theme.font_bold{font_size: 9.75}}}
    let DeskSmall = DeskText{draw_text +: {color: d_muted text_style: theme.font_regular{font_size: 8.25 line_spacing: 1.3}}}
    let DeskButton = ButtonFlat{height: 28 margin: 0 padding: Inset{left: 12 right: 12} text: ""
        draw_bg +: {border_radius: 3.0 border_size: 1.0
            border_color: d_border border_color_hover: d_border border_color_down: d_border border_color_focus: d_border border_color_disabled: d_border
            color: d_button color_hover: mix(d_button, d_ink, 0.04) color_down: mix(d_button, d_ink, 0.1) color_focus: d_button color_disabled: d_clear}
        draw_text +: {color: d_ink color_hover: d_ink color_down: d_ink color_focus: d_ink color_disabled: d_dim
            text_style: theme.font_regular{font_size: 9.75}}
    }
    let DeskDefault = DeskButton{
        draw_bg +: {border_size: 0.0 color: d_primary color_hover: #x006edb color_down: #x0060c0 color_focus: d_primary color_disabled: d_selected}
        draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff color_focus: #xffffff}
    }
    // A fallback's link beside the buttons, as Software Update's "More Info…": text in
    // the accent, no bezel.
    let DeskLink = ButtonFlat{height: 28 margin: 0 padding: 0 text: ""
        draw_bg +: {border_size: 0.0 color: d_clear color_hover: d_clear color_down: d_clear color_focus: d_clear color_disabled: d_clear}
        draw_text +: {color: d_link color_hover: d_link color_down: mix(d_link, d_ink, 0.3) color_focus: d_link color_disabled: d_dim
            text_style: theme.font_regular{font_size: 9.75}}
    }
    // The Mac's links under the status words, in their size.
    let DeskMore = DeskLink{height: 18 draw_text +: {text_style: theme.font_regular{font_size: 8.25}}}
    let DeskGroup = RoundedView{width: Fill height: Fit flow: Down show_bg: true
        draw_bg +: {color: d_surface border_radius: 5.0 border_size: 1.0 border_color: d_border}}
    let DeskRow = View{width: Fill height: 44 flow: Right align: Align{y: 0.5} spacing: 12 padding: Inset{left: 14 right: 14}}
    let DeskInset = SolidView{width: Fill height: 1 margin: Inset{left: 14 right: 14} draw_bg.color: d_border}
    // A mark on the pane's icon: the check of an up-to-date Mac, a failure's warning.
    let DeskMarkOk = View{visible: false width: 18 height: 18 show_bg: true
        draw_bg +: {
            pixel: fn(){
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.circle(9.0, 9.0, 8.5)
                sdf.fill(#xffffff)
                sdf.circle(9.0, 9.0, 7.5)
                sdf.fill(#x1f8a3d)
                sdf.move_to(5.4, 9.3)
                sdf.line_to(8.0, 11.9)
                sdf.line_to(12.7, 6.6)
                sdf.stroke(#xffffff, 1.6)
                return sdf.result
            }
        }
    }
    let DeskMarkWarn = View{visible: false width: 18 height: 18 show_bg: true
        draw_bg +: {
            pixel: fn(){
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.circle(9.0, 9.0, 8.5)
                sdf.fill(#xffffff)
                sdf.circle(9.0, 9.0, 7.5)
                sdf.fill(#xd9480f)
                sdf.move_to(9.0, 4.8)
                sdf.line_to(9.0, 10.0)
                sdf.stroke(#xffffff, 1.8)
                sdf.circle(9.0, 12.8, 1.1)
                sdf.fill(#xffffff)
                return sdf.result
            }
        }
    }
    // A download's progress: `fraction` from 0 to 1.
    // A Mac's spinner: twelve ticks and a brightness that steps round them.
    let DeskSpinner = View{visible: false width: 16 height: 16 show_bg: true
        draw_bg +: {
            color: uniform(d_muted)
            pixel: fn() {
                let center = self.rect_size * 0.5
                let p = self.pos * self.rect_size - center
                let outer = min(center.x, center.y)
                let tick = floor(atan2(p.y, p.x) / 6.2831853 * 12.0 + 0.5)
                let angle = tick / 12.0 * 6.2831853
                let dir = vec2(cos(angle), sin(angle))
                let half = max(outer * 0.09, 0.65)
                let along = dot(p, dir)
                let across = dot(p, vec2(-dir.y, dir.x))
                let d = length(vec2(along - clamp(along, outer * 0.5, outer - half), across)) - half
                let step_now = floor(self.draw_pass.time * 12.0)
                let lit = fract((tick - step_now - 1.0) / 12.0 + 0.001)
                let a = clamp(0.5 - d, 0.0, 1.0) * mix(0.22, 1.0, lit * 12.0 / 11.0)
                return vec4(self.color.rgb * a, a)
            }
        }
    }
    let DeskMeter = View{visible: false width: Fill height: 6 margin: Inset{top: 5 bottom: 3} show_bg: true
        draw_bg +: {
            fraction: instance(0.0)
            track: uniform(d_track)
            done: uniform(d_primary)
            pixel: fn(){
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 1.5)
                sdf.fill(self.track)
                sdf.box(0.0, 0.0, max(self.rect_size.y, self.rect_size.x * self.fraction), self.rect_size.y, 1.5)
                sdf.fill(self.done)
                return sdf.result
            }
        }
    }
    // The Mac's pop-up button, at one fixed width whichever channel it shows.
    // The current item is marked by its check alone (the shader's check sits at
    // x = 8, the text clears it), and its menu hangs left by that column, so the
    // current item's words lie over the button's (`popup_words_over`).
    let DeskPickerItem = PopupMenuItem{
        width: Fill height: 24 padding: Inset{left: 20 right: 10}
        draw_text +: {color: d_ink color_hover: #xffffff color_active: d_ink text_style: theme.font_regular{font_size: 9.75}}
        draw_bg +: {border_size: 0.0 border_radius: 2.5 color: d_clear color_active: d_clear color_hover: d_primary mark_color_active: d_ink mark_color_highlight: #xffffff}
    }
    let DeskPicker = DropDown{
        width: 170 height: 28 margin: 0
        padding: Inset{left: 8 right: 24 top: 0 bottom: 0}
        align: Align{x: 0.0 y: 0.5}
        labels: ["Stable", "Release Candidate", "Preview"]
        popup_words_over: true
        popup_menu +: {
            // From 16 points left of the button to 4 past its right: it covers the button.
            width: 190 padding: 4
            menu_item: DeskPickerItem{}
            draw_bg +: {border_size: 1.0 border_radius: 3.0 color: d_button border_color: d_border border_color_2: d_border}
        }
        draw_text +: {
            color: d_ink color_hover: d_ink color_focus: d_ink color_down: d_ink color_disabled: d_dim
            text_style: theme.font_regular{font_size: 9.75}
        }
        draw_bg +: {
            color: d_button color_hover: mix(d_button, d_ink, 0.04) color_disabled: d_clear
            border_color: mix(d_border, d_ink, 0.12) arrow_color: d_ink arrow_color_disabled: d_dim
            pixel: fn(){
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 3.0)
                sdf.fill_keep(mix(mix(self.color, self.color_hover, self.hover), self.color_disabled, self.disabled))
                sdf.stroke(self.border_color, 1.0)
                let arrow = mix(self.arrow_color, self.arrow_color_disabled, self.disabled)
                let c = vec2(self.rect_size.x - 13.0, self.rect_size.y * 0.5)
                sdf.move_to(c.x - 3.0, c.y - 1.5)
                sdf.line_to(c.x, c.y - 4.5)
                sdf.line_to(c.x + 3.0, c.y - 1.5)
                sdf.stroke(arrow, 1.2)
                sdf.move_to(c.x - 3.0, c.y + 1.5)
                sdf.line_to(c.x, c.y + 4.5)
                sdf.line_to(c.x + 3.0, c.y + 1.5)
                sdf.stroke(arrow, 1.2)
                return sdf.result
            }
        }
    }
    let DeskBody = ScrollYView{width: Fill height: Fill flow: Down align: Align{x: 0.5}
        body := View{width: Fill max_width: 620 height: Fit flow: Down spacing: 10 padding: Inset{left: 20 right: 20 top: 20 bottom: 20}
            // A Mac's Software Update: the step's button at the end of the status row,
            // the box as tall as its words, and what else there is to read as links
            // under them, as its "More Info…" (the Release Notes, the installer fallback).
            if desk_mac {
                DeskGroup{
                    View{width: Fill height: Fit flow: Right spacing: 14 align: Align{y: 0.5} padding: 14
                        View{width: 48 height: 48 flow: Overlay align: Align{x: 1.0 y: 1.0}
                            AppIcon{width: 48 height: 48 name: "updater"}
                            mark_ok := DeskMarkOk{}
                            mark_warn := DeskMarkWarn{}
                        }
                        View{width: Fill height: Fit flow: Down spacing: 3
                            current := DeskBold{text: "Reading this installation…"}
                            status := DeskSmall{}
                            checked := DeskSmall{}
                            meter := DeskMeter{}
                            links := View{visible: false width: Fill height: Fit flow: Right spacing: 16
                                // A development build's Check for Updates, under the words: the
                                // way to a release is the one button beside them.
                                check_more := DeskMore{text: "Check for Updates" visible: false}
                                notes := DeskMore{text: "Release Notes" visible: false}
                                manual := DeskMore{text: "Open Installer Manually" visible: false}
                            }
                        }
                        mac_actions := View{width: Fit height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                            busy_mark := DeskSpinner{margin: Inset{right: 4}}
                            notes_plain := DeskButton{visible: false}
                            cancel := DeskButton{text: "Cancel" visible: false}
                            check_plain := DeskButton{text: "Check for Updates" visible: false}
                            check := DeskDefault{text: "Check for Updates" visible: false}
                            download := DeskDefault{text: "Download Update" visible: false}
                            retry := DeskDefault{text: "Try Again" visible: false}
                            install := DeskDefault{text: "Open Installer" visible: false}
                            notes_lead := DeskDefault{visible: false}
                        }
                    }
                }
            } else {
                DeskGroup{
                    View{width: Fill height: Fit flow: Right spacing: 14 align: Align{y: 0.5} padding: Inset{left: 14 right: 14 top: 14 bottom: 10}
                        View{width: 48 height: 48 flow: Overlay align: Align{x: 1.0 y: 1.0}
                            AppIcon{width: 48 height: 48 name: "updater"}
                            mark_ok := DeskMarkOk{}
                            mark_warn := DeskMarkWarn{}
                        }
                        View{width: Fill height: Fit flow: Down spacing: 3
                            current := DeskBold{text: "Reading this installation…"}
                            status := DeskSmall{}
                            // Under the words, not in the button row, where two buttons would wrap it.
                            checked := DeskSmall{}
                            meter := DeskMeter{}
                        }
                    }
                    View{width: Fill height: Fit flow: Right spacing: 8 align: Align{x: 1.0 y: 0.5} padding: Inset{left: 14 right: 14 bottom: 12}
                        // The fallback keeps out of the button row: a link at its start.
                        manual := DeskLink{text: "Open Installer Manually" visible: false}
                        View{width: Fill height: 1}
                        busy_mark := DeskSpinner{margin: Inset{right: 4}}
                        notes := DeskButton{text: "Release Notes" visible: false}
                        retry_install := DeskButton{text: "Refresh Installation Status" visible: false}
                        permission := DeskButton{text: "Allow Updates in Android Settings" visible: false}
                        cancel := DeskButton{text: "Cancel" visible: false}
                        check_plain := DeskButton{text: "Check for Updates" visible: false}
                        check := DeskDefault{text: "Check for Updates" visible: false}
                        download := DeskDefault{text: "Download Update" visible: false}
                        retry := DeskDefault{text: "Try Again" visible: false}
                        install := DeskDefault{text: "Open Installer" visible: false}
                        notes_lead := DeskDefault{visible: false}
                    }
                }
            }
            DeskGroup{
                DeskRow{
                    DeskText{width: Fit text: "Release channel"}
                    View{width: Fill height: 1}
                    channel := DeskPicker{}
                }
                DeskInset{}
                DeskRow{
                    DeskText{width: Fit text: "Source"}
                    View{width: Fill height: 1}
                    source := DeskText{width: Fit text: "Official releases from OctoSense" draw_text +: {color: d_muted}}
                }
            }
            View{width: Fill height: Fit padding: Inset{left: 14 right: 14}
                detail := DeskSmall{}
            }
        }
    }
    let TouchBody = View{width: Fill height: Fill flow: Down padding: 24 spacing: 18
        title := Label { text: "OctoSense Updates" draw_text.text_style.font_size: 25 }
        current := Label { width: Fill text: "Reading this installation…" draw_text.wrap: Words }
        ScrollYView {
            width: Fill height: Fill
            body := View {
                width: Fill height: Fit flow: Down spacing: 16
                channel := Button { height: 44 text: "Channel: Preview" }
                source := Label { width: Fill text: "Official releases from OctoSense-org / OctoSense on GitHub." draw_text.wrap: Words }
                status := Label { width: Fill text: "Check for an update when you are ready." draw_text.wrap: Words }
                offer := Label { width: Fill text: "" draw_text.wrap: Words }
                check := Button { width: Fill height: 44 text: "Check for updates" }
                download := Button { width: Fill height: 44 text: "Download update" visible: false }
                cancel := Button { width: Fill height: 44 text: "Cancel download" visible: false }
                install := Button { width: Fill height: 44 text: "Open installer" visible: false }
                manual := Button { width: Fill height: 44 text: "Open installer manually" visible: false }
                permission := Button { width: Fill height: 44 text: "Allow updates in Android Settings" visible: false }
                retry_install := Button { width: Fill height: 44 text: "Refresh installation status" visible: false }
                notes := Button { width: Fill height: 44 text: "Release notes" visible: false }
                detail := Label { width: Fill text: "Updates are downloaded and verified before you choose to install. Save your work before replacing or restarting OctoSense." draw_text.wrap: Words }
            }
        }
    }
    mod.widgets.UpdaterView = set_type_default() do #(UpdaterView::register_widget(vm)) {
        ..mod.widgets.RectView
        width: Fill height: Fill
        draw_bg.color: theme.color_bg_app
        flow: Down
        pointer: desk
        if desk { DeskBody{} } else { TouchBody{} }
    }
}

/// What a desktop's pane is showing. A phone's strings stay in `message`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Phase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    /// A development build can't update itself; a check still finds the newest release.
    Development,
    NoRelease,
    Available,
    Downloading(u64, u64),
    Ready,
    /// The download is being checked again, before a restart or an installer.
    Preparing { restart: bool },
    /// Only a Mac restarts into an update (`Completed::MacPrepared`).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    RestartWaiting,
    Opened,
    Failed(Attempt),
}

/// What a failure's Try Again repeats.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Attempt {
    Check,
    Download,
    Install,
}

/// The facts a desktop's pane is worded from.
struct Facts<'a> {
    phase: Phase,
    supported: bool,
    /// The installed release in words; `None` on a development build.
    installed: Option<String>,
    /// The newest release a check found, in words.
    newest: Option<String>,
    /// The offered download, in bytes.
    size: u64,
    channel: &'a str,
    /// macOS can replace the app itself (`macos::available`).
    restart: bool,
    /// Says something of the moment in place of the state's own sentence.
    note: Option<&'a str>,
    /// The Mac's Software Update pane (DeskBody's `desk_mac`): its words end on more than one word.
    mac: bool,
}

/// What a desktop's pane says and offers. At most one default button (`check`,
/// `download`, `retry`, `install`, or the link when `link_leads`) shows at a
/// time, rightmost.
#[derive(Debug, Default, PartialEq)]
struct Pane {
    headline: String,
    status: String,
    /// Shows the "Last checked" line.
    checked: bool,
    mark_ok: bool,
    mark_warn: bool,
    spinner: bool,
    meter: Option<f64>,
    link: Option<&'static str>,
    /// The link is the step that moves a person on: the default button, and
    /// Check for Updates a plain one.
    link_leads: bool,
    /// Check for Updates is a plain button and nothing is the default: nothing
    /// waits (up to date, an installer opened), or the next step is another
    /// control (no release on this channel: the channel pop-up), as Software
    /// Update shows a plain button once the Mac is up to date.
    check_plain: bool,
    manual: bool,
    cancel: Option<&'static str>,
    check: bool,
    download: bool,
    retry: bool,
    install: Option<&'static str>,
}

fn pane_words(f: &Facts) -> Pane {
    let newest = f.newest.as_deref().unwrap_or_default();
    let megabytes = |bytes: u64| format!("{:.1} MB", bytes as f64 / 1_000_000.0);
    let mut pane = if !f.supported {
        Pane {
            headline: "Updates Aren’t Available Here".into(),
            status: "OctoSense can’t update itself on this computer. New versions are on its releases page.".into(),
            link: Some("View Releases"),
            ..Pane::default()
        }
    } else {
        match f.phase {
            Phase::Idle => Pane {
                headline: match &f.installed {
                    Some(version) => format!("OctoSense {version}"),
                    None => "OctoSense Development Build".into(),
                },
                status: if f.installed.is_some() {
                    String::new()
                } else {
                    "A development build can’t install updates, but it can show you the newest release on this channel.".into()
                },
                checked: true,
                check: true,
                ..Pane::default()
            },
            Phase::Checking => Pane {
                headline: "Checking for Updates…".into(),
                checked: true,
                spinner: true,
                cancel: Some("Cancel"),
                ..Pane::default()
            },
            Phase::UpToDate => Pane {
                headline: "OctoSense Is Up to Date".into(),
                status: if f.mac {
                    format!("{} is the newest {} version.", f.installed.as_deref().unwrap_or("This"), f.channel)
                } else {
                    format!(
                        "{} is the newest version on the {} channel.",
                        f.installed.as_deref().unwrap_or("This"),
                        f.channel
                    )
                },
                checked: true,
                mark_ok: true,
                check: true,
                check_plain: true,
                ..Pane::default()
            },
            Phase::Development => Pane {
                headline: "This Is a Development Build".into(),
                status: format!(
                    "It can’t update itself. The newest version on the {} channel is {newest}; install it to get updates here.",
                    f.channel
                ),
                checked: true,
                link: Some("Get Release Build…"),
                link_leads: true,
                check: true,
                ..Pane::default()
            },
            Phase::NoRelease => Pane {
                headline: "No Releases on This Channel".into(),
                status: if f.channel == "Preview" {
                    "OctoSense has no release for this computer yet.".into()
                } else {
                    format!(
                        "OctoSense has no {} release for this computer yet. Choose another channel to see test versions.",
                        f.channel
                    )
                },
                checked: true,
                check: true,
                check_plain: true,
                ..Pane::default()
            },
            Phase::Available => Pane {
                headline: format!("OctoSense {newest} Is Available"),
                status: match &f.installed {
                    Some(version) => format!("You have {version}. The download is {}.", megabytes(f.size)),
                    None => format!("The download is {}.", megabytes(f.size)),
                },
                checked: true,
                link: Some("Release Notes"),
                download: true,
                ..Pane::default()
            },
            Phase::Downloading(received, total) => Pane {
                headline: format!("Downloading OctoSense {newest}…"),
                status: format!("{} of {}", megabytes(received), megabytes(total)),
                meter: Some(if total == 0 { 0.0 } else { (received as f64 / total as f64).clamp(0.0, 1.0) }),
                cancel: Some("Cancel"),
                ..Pane::default()
            },
            Phase::Ready => Pane {
                headline: format!("OctoSense {newest} Is Ready to Install"),
                status: if f.restart {
                    "The download is verified. Save your work first: OctoSense asks your apps to close, then restarts. Your profile stays as it is.".into()
                } else {
                    "The download is verified. Save your work first: installing may restart OctoSense.".into()
                },
                link: Some("Release Notes"),
                manual: f.restart,
                install: Some(if f.restart { "Install & Restart" } else { "Open Installer" }),
                ..Pane::default()
            },
            Phase::Preparing { restart: true } => Pane {
                headline: "Preparing the Update…".into(),
                status: "OctoSense checks the download again, then asks your apps to close.".into(),
                spinner: true,
                cancel: Some("Cancel"),
                ..Pane::default()
            },
            Phase::Preparing { restart: false } => Pane {
                headline: "Opening the Installer…".into(),
                status: "OctoSense checks the download again before it opens.".into(),
                spinner: true,
                cancel: Some("Cancel"),
                ..Pane::default()
            },
            Phase::RestartWaiting => Pane {
                headline: "Waiting to Restart…".into(),
                status: "Your apps are saving and closing. OctoSense restarts when they finish.".into(),
                spinner: true,
                cancel: Some("Cancel Restart"),
                ..Pane::default()
            },
            Phase::Opened => Pane {
                headline: "Installer Opened".into(),
                check: true,
                check_plain: true,
                ..Pane::default()
            },
            Phase::Failed(attempt) => Pane {
                headline: match attempt {
                    Attempt::Check => "Can’t Check for Updates",
                    Attempt::Download => "Can’t Download the Update",
                    Attempt::Install => "Can’t Install the Update",
                }
                .into(),
                checked: attempt == Attempt::Check,
                mark_warn: true,
                link: (attempt != Attempt::Check).then_some("Release Notes"),
                manual: attempt == Attempt::Install && f.restart,
                retry: attempt != Attempt::Install,
                // The same button as Install & Restart: Try Again acts only on a real click too.
                install: (attempt == Attempt::Install).then_some("Try Again"),
                ..Pane::default()
            },
        }
    };
    if let Some(note) = f.note {
        pane.status = note.to_owned();
    }
    pane
}

/// A release's version as a person reads it: `0.1.0-rc.6` is "0.1.0 RC 6".
fn plain_version(version: &str) -> String {
    let Some((base, pre)) = version.split_once('-') else {
        return version.to_owned();
    };
    let mut words = vec![base.to_owned()];
    for part in pre.split('.') {
        words.push(match part {
            "rc" => "RC".to_owned(),
            other => {
                let mut letters = other.chars();
                match letters.next() {
                    Some(first) => first.to_uppercase().chain(letters).collect(),
                    None => String::new(),
                }
            }
        });
    }
    words.join(" ")
}

/// A failure's sentence from the updater's own words (`octosense_updater::Error`).
/// Known system error codes and process statuses are not useful instructions;
/// the pane's headline identifies which step failed and its button retries it.
fn plain_failure(error: &str) -> String {
    const WORDS: [(&str, &str); 7] = [
        ("Could not reach the public release server", "OctoSense couldn’t reach its release server. Check your internet connection, then try again."),
        ("The release download timed out", "The connection timed out. Try again when you’re back online."),
        ("GitHub temporarily limited", "Too many update checks came from this network. Try again in a little while."),
        ("Release metadata was rejected", "The release couldn’t be verified, so nothing was downloaded."),
        ("Update verification failed", "The download couldn’t be verified, so it can’t be installed. Nothing was changed."),
        ("Update storage is unavailable", "OctoSense couldn’t save the update. Check that your disk has free space."),
        ("No updater package is supported", "No update is packaged for this computer."),
    ];
    if let Some((_, words)) = WORDS
        .iter()
        .find(|(start, _)| error.starts_with(start))
    {
        return (*words).to_owned();
    }
    let detail = error.to_ascii_lowercase();
    if detail.contains("(os error ")
        || detail.contains("exit status:")
        || detail.contains("signal:")
        || detail.starts_with("could not start update worker:")
    {
        return "OctoSense couldn’t complete the update request. Try again.".into();
    }
    error.replace("Open installer manually", "Open Installer Manually")
}

/// When the last check finished: "Last checked: Today at 3:45 PM".
fn last_checked_words(checked: Option<i64>, now: i64, utc_offset: i64) -> String {
    let Some(checked) = checked else {
        return "Last checked: Never".into();
    };
    const DAY: i64 = 86_400;
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let local = checked + utc_offset;
    let day = local.div_euclid(DAY);
    let minutes = local.rem_euclid(DAY) / 60;
    let (hour, minute) = (minutes / 60, minutes % 60);
    let (clock_hour, half) = match hour {
        0 => (12, "AM"),
        1..=11 => (hour, "AM"),
        12 => (12, "PM"),
        _ => (hour - 12, "PM"),
    };
    let date = match (now + utc_offset).div_euclid(DAY) - day {
        0 => "Today".to_owned(),
        1 => "Yesterday".to_owned(),
        _ => {
            // Days since 1970 to a civil date (Howard Hinnant's algorithm).
            let z = day + 719_468;
            let era = z.div_euclid(146_097);
            let doe = z.rem_euclid(146_097);
            let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let date = doy - (153 * mp + 2) / 5 + 1;
            let month = if mp < 10 { mp + 3 } else { mp - 9 };
            let year = yoe + era * 400 + i64::from(month <= 2);
            format!("{} {date}, {year}", MONTHS[(month - 1) as usize])
        }
    };
    format!("Last checked: {date} at {clock_hour}:{minute:02} {half}")
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

const LAST_CHECKED_FILE: &str = "last-checked";

/// The pop-up's order: `UpdaterView::channel` indexes it.
const CHANNEL_NAMES: [&str; 3] = ["Stable", "Release Candidate", "Preview"];
/// Under the channel's row: what it brings (`docs/updater.md`).
const CHANNEL_NOTES: [&str; 3] = [
    "This channel has finished releases only.",
    "This channel has finished releases and release candidates.",
    "This channel has every release, including early test versions.",
];
const PROMISE: &str = "Updates are verified before you install them. Changing channels never installs an older version.";

/// This build as a test reads it (`pretend`): its own identity on a release,
/// whose tag is compiled in, whatever the hook says. Otherwise: the release it
/// was told to read as, whether it can't update itself at all, and whether to
/// offer Install & Restart.
fn test_identity(built: BuildIdentity, hook: Option<&str>) -> (BuildIdentity, bool, bool) {
    let hook = hook.filter(|_| built.version().is_none());
    let (tag, restart) = match hook {
        Some(hook) => hook.strip_suffix("+restart").map_or((hook, false), |tag| (tag, true)),
        None => return (built, false, false),
    };
    if tag == "unsupported" {
        return (built, true, false);
    }
    match BuildIdentity::from_tag(built.product(), Some(tag)) {
        Ok(identity) => (identity, false, restart),
        Err(_) => (built, false, false),
    }
}

enum Completed {
    Checked(CheckResult),
    Downloaded(VerifiedArtifact),
    Opened(String),
    AndroidReady(PathBuf, String),
    #[cfg(target_os = "macos")]
    MacPrepared(octosense_updater::macos::Prepared),
}
enum WorkerEvent {
    Progress(u64, u64),
    Finished(Box<Result<Completed, String>>),
}
struct Worker {
    receive: mpsc::Receiver<WorkerEvent>,
    cancel: Cancellation,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn worker(
    run: impl FnOnce(&Cancellation, &mpsc::SyncSender<WorkerEvent>) -> Result<Completed, String>
        + Send
        + 'static,
) -> Result<Worker, String> {
    if BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("Another update operation is still finishing. Try again shortly.".into());
    }
    let (send, receive) = mpsc::sync_channel(16);
    let cancel = Cancellation::default();
    let token = cancel.clone();
    let spawned = std::thread::Builder::new()
        .name("octosense-updater".into())
        .spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&token, &send)))
                    .unwrap_or_else(|_| {
                        Err("Update operation stopped unexpectedly. You can retry.".into())
                    });
            BUSY.store(false, Ordering::Release);
            if let Err(mpsc::SendError(WorkerEvent::Finished(result))) =
                send.send(WorkerEvent::Finished(Box::new(result)))
            {
                // The window closed just as a download finished. No view owns
                // this unused artifact, so do not leave it in private storage.
                if let Ok(Completed::Downloaded(artifact)) = *result {
                    let _ = artifact.discard();
                }
            }
            makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
        });
    if let Err(error) = spawned {
        BUSY.store(false, Ordering::Release);
        return Err(format!("Could not start update worker: {error}"));
    }
    Ok(Worker { receive, cancel })
}

#[derive(Script, ScriptHook, Widget)]
pub struct UpdaterView {
    #[deref]
    view: View,
    /// A desktop style's sheet: the Mac's Software Update pane (`theme.ui_pointer`).
    #[live]
    pointer: bool,
    /// The Mac's pane (`mac_actions` is in the tree), read at each refresh.
    #[rust]
    mac: bool,
    #[rust]
    initialized: bool,
    #[rust]
    channel: usize,
    #[rust]
    identity: Option<BuildIdentity>,
    #[rust]
    cache: Option<PathBuf>,
    #[rust]
    offer: Option<Offer>,
    #[rust]
    artifact: Option<VerifiedArtifact>,
    #[rust]
    task: Option<Worker>,
    #[rust]
    message: String,
    #[rust]
    android_request: u64,
    #[rust]
    android_standalone: bool,
    #[rust]
    android_can_install: bool,
    #[rust]
    android_install_pending: bool,
    #[rust]
    install_opened: bool,
    /// Whether the press of the Install and Allow buttons was a real tap.
    #[rust]
    install_down: bool,
    #[rust]
    permission_down: bool,
    #[rust]
    mac_restart: bool,
    #[rust]
    restart_waiting: bool,
    /// A desktop's pane (`pane_words`). A phone reads `message`.
    #[rust]
    phase: Phase,
    /// Says something of the moment in place of the phase's own sentence.
    #[rust]
    note: Option<String>,
    /// What a development build's check found: named, never downloaded.
    #[rust]
    dev_release: Option<Offer>,
    #[rust]
    last_checked: Option<i64>,
    /// While something is under way, a spinner's frames.
    #[rust]
    spin: NextFrame,
    /// The window the pane is in, which a Mac's pane sizes to its words.
    #[rust]
    windows: Option<ModuleWindows>,
    /// The height of the words the window was last fitted to.
    #[rust]
    fitted: f64,
}

impl UpdaterView {
    /// A Mac's update window is as tall as what it shows: 20 points under its last row, in
    /// every state. Asked once for each height of the words, so a person's own size stays
    /// until the words change.
    fn fit_window(&mut self, cx: &mut Cx) {
        let Some(windows) = self.windows.as_ref().filter(|windows| self.mac && windows.is_supported()) else { return };
        let pane = self.view.area().rect(cx);
        let body = self.view.widget(cx, ids!(body)).area().rect(cx);
        if pane.size.y <= 0.0 || body.size.y <= 0.0 || (body.size.y - self.fitted).abs() < 0.5 {
            return;
        }
        self.fitted = body.size.y;
        let dy = body.size.y - pane.size.y;
        if dy.abs() >= 0.5 {
            windows.resize_by(0.0, dy);
        }
    }

    fn product() -> Product {
        if cfg!(target_os = "android") {
            Product::Home
        } else {
            Product::Desktop
        }
    }

    fn android_command(&mut self, cx: &mut Cx, operation: &str, mut value: serde_json::Value) {
        let id = REQUEST.fetch_add(1, Ordering::Relaxed);
        self.android_request = id;
        value["id"] = id.into();
        value["operation"] = operation.into();
        cx.android_integration("home_updater.command", &value.to_string());
    }

    fn initialize(&mut self, cx: &mut Cx) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let (identity, unsupported, restart) = test_identity(
            BuildIdentity::from_env(Self::product()),
            PRETEND.get().map(String::as_str),
        );
        self.channel = match identity.default_channel() {
            Channel::Stable => 0,
            Channel::ReleaseCandidate => 1,
            Channel::Preview => 2,
        };
        self.view.label(cx, ids!(current)).set_text(
            cx,
            &format!(
                "Installed: {}",
                identity
                    .version()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "Development build (no release version)".into())
            ),
        );
        self.identity = Some(identity);
        self.cache = CACHE.get().cloned().filter(|_| !unsupported);
        #[cfg(target_os = "macos")]
        { self.mac_restart = octosense_updater::macos::available(); }
        self.mac_restart |= restart;
        if self.pointer {
            let line = self.cache.as_ref()
                .and_then(|cache| std::fs::read_to_string(cache.join(LAST_CHECKED_FILE)).ok())
                .unwrap_or_default();
            let mut fields = line.split_whitespace();
            self.last_checked = fields.next().and_then(|time| time.parse().ok());
            // Opened again, the window says what the last check found, as long
            // as that was this installation on this channel.
            let found = fields.collect::<Vec<_>>();
            let here = self.identity.as_ref().and_then(|identity| identity.version()).map(ToString::to_string);
            if found.len() == 3 && found[0] == "current" && found[1] == self.channel.to_string() && Some(found[2]) == here.as_deref() {
                self.phase = Phase::UpToDate;
            }
        }
        self.message = "Check for an update when you are ready.".into();
        if cfg!(target_os = "android") {
            self.message = "Checking this Android installation…".into();
            self.android_command(cx, "update_info", serde_json::json!({}));
        } else if Platform::current(Self::product()).is_none() {
            self.message = "In-app updates are not available for this platform yet.".into();
        }
        self.refresh(cx);
    }

    /// A desktop's pane as it stands now.
    fn pane(&self) -> Pane {
        let android = cfg!(target_os = "android");
        let newest = self.offer.as_ref().or(self.dev_release.as_ref());
        pane_words(&Facts {
            phase: self.phase,
            supported: Platform::current(Self::product()).is_some()
                && self.cache.is_some()
                && (!android || self.android_standalone),
            installed: self
                .identity
                .as_ref()
                .and_then(BuildIdentity::version)
                .map(|version| plain_version(&version.to_string())),
            newest: newest.map(|offer| plain_version(&offer.version().to_string())),
            size: self.offer.as_ref().map_or(0, Offer::size),
            channel: CHANNEL_NAMES[self.channel],
            restart: self.mac_restart,
            // Android's own words for its installer steps, whichever tree it draws.
            note: if android { Some(self.message.as_str()) } else { self.note.as_deref() },
            mac: self.mac,
        })
    }

    fn refresh_desk(&mut self, cx: &mut Cx) {
        self.mac = !self.view.widget(cx, ids!(mac_actions)).is_empty();
        let pane = self.pane();
        self.view.label(cx, ids!(current)).set_text(cx, &pane.headline);
        let status = self.view.label(cx, ids!(status));
        status.set_text(cx, &pane.status);
        status.set_visible(cx, !pane.status.is_empty());
        let checked = self.view.label(cx, ids!(checked));
        checked.set_visible(cx, pane.checked);
        if pane.checked {
            checked.set_text(
                cx,
                &last_checked_words(
                    self.last_checked,
                    unix_now(),
                    makepad_widgets::makepad_platform::script::timer::script_local_utc_offset_secs(),
                ),
            );
        }
        self.view.widget(cx, ids!(mark_ok)).set_visible(cx, pane.mark_ok);
        self.view.widget(cx, ids!(mark_warn)).set_visible(cx, pane.mark_warn);
        self.view.widget(cx, ids!(busy_mark)).set_visible(cx, pane.spinner);
        let mut meter = self.view.widget(cx, ids!(meter));
        meter.set_visible(cx, pane.meter.is_some());
        if let Some(fraction) = pane.meter {
            let fraction = fraction as f32;
            script_apply_eval!(cx, meter, {draw_bg +: {fraction: #(fraction)}});
        }
        let android = cfg!(target_os = "android");
        let shown = |view: &View, cx: &mut Cx, id: &[LiveId], on: bool| {
            view.widget(cx, id).set_visible(cx, on);
        };
        // A Mac's pane (`mac_actions`, DeskBody's `desk_mac` branch) keeps one button beside its
        // words, as Software Update does: when the way to a release leads, Check for Updates is a
        // link under them, so the words keep their width.
        let mac = self.mac;
        let check_under = mac && pane.check && pane.link_leads;
        shown(&self.view, cx, ids!(check), pane.check && !pane.link_leads && !pane.check_plain);
        shown(&self.view, cx, ids!(check_plain), pane.check && (pane.link_leads || pane.check_plain) && !check_under);
        shown(&self.view, cx, ids!(check_more), check_under);
        shown(&self.view, cx, ids!(download), pane.download);
        shown(&self.view, cx, ids!(retry), pane.retry);
        shown(&self.view, cx, ids!(manual), pane.manual);
        // The release notes are a link under the words there, or the row's button when they are
        // the only step.
        let lone = !(pane.check || pane.download || pane.retry || pane.cancel.is_some() || pane.install.is_some());
        let notes = pane.link.is_some() && !pane.link_leads;
        shown(&self.view, cx, ids!(notes), notes && !(mac && lone));
        shown(&self.view, cx, ids!(notes_plain), notes && mac && lone);
        shown(&self.view, cx, ids!(links), mac && ((notes && !lone) || pane.manual || check_under));
        self.view.button(cx, ids!(notes_plain)).set_text(cx, pane.link.unwrap_or_default());
        shown(&self.view, cx, ids!(notes_lead), pane.link.is_some() && pane.link_leads);
        self.view.button(cx, ids!(notes_lead)).set_text(cx, pane.link.unwrap_or_default());
        shown(&self.view, cx, ids!(cancel), pane.cancel.is_some());
        shown(&self.view, cx, ids!(install), pane.install.is_some());
        self.view.button(cx, ids!(notes)).set_text(cx, pane.link.unwrap_or_default());
        self.view.button(cx, ids!(cancel)).set_text(cx, pane.cancel.unwrap_or_default());
        self.view.button(cx, ids!(install)).set_text(cx, pane.install.unwrap_or_default());
        // Android's steps, should its Home ever draw this tree.
        shown(
            &self.view,
            cx,
            ids!(permission),
            android && self.artifact.is_some() && !self.android_can_install && self.task.is_none() && !self.android_install_pending,
        );
        shown(
            &self.view,
            cx,
            ids!(retry_install),
            android && (self.android_install_pending || self.artifact.is_some()),
        );
        let settled = self.task.is_none() && !self.restart_waiting && !self.android_install_pending;
        let picker = self.view.drop_down(cx, ids!(channel));
        picker.set_selected_item(cx, self.channel);
        self.view.widget(cx, ids!(channel)).set_disabled(cx, !settled || !self.pane_supported());
        self.view
            .label(cx, ids!(detail))
            .set_text(cx, &format!("{} {PROMISE}", CHANNEL_NOTES[self.channel]));
        self.view.redraw(cx);
    }

    /// The release's page (its notes), or on a desktop that can't update, the page
    /// of every release.
    fn open_notes(&self, cx: &mut Cx) {
        match self.offer.as_ref().or(self.dev_release.as_ref()) {
            Some(offer) => cx.open_url(&offer.release_url(), OpenUrlInPlace::No),
            None if self.pointer => cx.open_url(octosense_updater::RELEASES_WEB, OpenUrlInPlace::No),
            None => {}
        }
    }

    fn pane_supported(&self) -> bool {
        Platform::current(Self::product()).is_some()
            && self.cache.is_some()
            && (!cfg!(target_os = "android") || self.android_standalone)
    }

    fn refresh(&mut self, cx: &mut Cx) {
        if self.pointer {
            self.refresh_desk(cx);
            return;
        }
        let busy = self.task.is_some() || self.restart_waiting;
        let supported = Platform::current(Self::product()).is_some()
            && self.cache.is_some()
            && (!cfg!(target_os = "android") || self.android_standalone);
        self.view
            .label(cx, ids!(status))
            .set_text(cx, &self.message);
        self.view.button(cx, ids!(channel)).set_text(
            cx,
            match self.channel {
                0 => "Channel: Stable",
                1 => "Channel: Release candidate",
                _ => "Channel: Preview (all test releases)",
            },
        );
        self.view
            .button(cx, ids!(channel))
            .set_enabled(cx, !busy && !self.android_install_pending);
        self.view
            .button(cx, ids!(check))
            .set_enabled(cx, supported && !busy && !self.android_install_pending);
        self.view
            .button(cx, ids!(download))
            .set_visible(cx, self.offer.is_some() && self.artifact.is_none() && !busy);
        self.view.button(cx, ids!(cancel)).set_visible(cx, busy);
        self.view.button(cx, ids!(cancel)).set_text(cx, if self.restart_waiting { "Cancel restart" } else { "Cancel" });
        self.view
            .button(cx, ids!(cancel))
            .set_enabled(cx, busy && !self.android_install_pending);
        self.view.button(cx, ids!(install)).set_visible(
            cx,
            self.artifact.is_some() && !busy && !self.android_install_pending,
        );
        self.view.button(cx, ids!(install)).set_text(
            cx,
            if cfg!(target_os = "android") {
                "Review and install update"
            } else if self.mac_restart {
                "Install & Restart"
            } else {
                "Open verified installer"
            },
        );
        self.view.button(cx, ids!(manual)).set_visible(cx, self.mac_restart && self.artifact.is_some() && !busy);
        self.view.button(cx, ids!(permission)).set_visible(
            cx,
            cfg!(target_os = "android")
                && self.artifact.is_some()
                && !self.android_can_install
                && !busy
                && !self.android_install_pending,
        );
        self.view.button(cx, ids!(retry_install)).set_visible(
            cx,
            cfg!(target_os = "android")
                && (self.android_install_pending || self.artifact.is_some()),
        );
        self.view
            .button(cx, ids!(notes))
            .set_visible(cx, self.offer.is_some());
        let description = self
            .offer
            .as_ref()
            .map(|offer| {
                format!(
                    "OctoSense {}\n{} · {:.1} MB",
                    offer.version(),
                    offer.asset_name(),
                    offer.size() as f64 / 1_000_000.0
                )
            })
            .unwrap_or_default();
        self.view.label(cx, ids!(offer)).set_text(cx, &description);
        self.view.redraw(cx);
    }

    fn start(&mut self, task: Result<Worker, String>) {
        match task {
            Ok(task) => self.task = Some(task),
            Err(error) => {
                self.failed(&error);
            }
        }
    }

    /// A desktop's pane after an operation failed: Try Again repeats it.
    fn failed(&mut self, error: &str) {
        let message = plain_failure(error);
        self.note = Some(message.clone());
        self.message = message;
        self.phase = Phase::Failed(match self.phase {
            Phase::Downloading(..) => Attempt::Download,
            Phase::Preparing { .. } => Attempt::Install,
            _ => Attempt::Check,
        });
    }

    fn retire_offer(&mut self) {
        if let Some(artifact) = self.artifact.take() {
            // A mounted DMG or installer may still need its source file.
            if !self.install_opened {
                let _ = artifact.discard();
            }
        }
        self.offer = None;
        self.install_opened = false;
    }

    fn check(&mut self) {
        if self.task.is_some() || self.android_install_pending {
            return;
        }
        let Some(cache) = self.cache.clone() else {
            return;
        };
        let Some(platform) = Platform::current(Self::product()) else {
            return;
        };
        if cfg!(target_os = "android") && !self.android_standalone {
            return;
        }
        let Some(identity) = self.identity.clone() else {
            return;
        };
        self.retire_offer();
        let channel = match self.channel {
            0 => Channel::Stable,
            1 => Channel::ReleaseCandidate,
            _ => Channel::Preview,
        };
        self.message = "Checking official releases…".into();
        self.phase = Phase::Checking;
        self.note = None;
        self.dev_release = None;
        let remember = self.pointer.then(|| cache.clone());
        let picked = self.channel;
        self.start(worker(move |cancel, _| {
            let client = Client::new(cache).map_err(|e| e.to_string())?;
            let result = client
                .check(&identity, channel, platform, cancel)
                .map_err(|e| e.to_string())?;
            if let Some(cache) = remember {
                // A line that can't be written just says less next time. The
                // first check can come before any download has made the folder.
                // When, and whether it found this version current on this channel.
                let found = match identity.version() {
                    Some(version) if result.status == CheckStatus::Current => format!(" current {picked} {version}"),
                    _ => String::new(),
                };
                if let Ok(folder) = octosense_updater::prepare_cache(&cache) {
                    let _ = std::fs::write(folder.join(LAST_CHECKED_FILE), format!("{}{found}", unix_now()));
                }
            }
            Ok(Completed::Checked(result))
        }));
    }

    fn download(&mut self) {
        if self.task.is_some() {
            return;
        }
        let (Some(cache), Some(offer)) = (self.cache.clone(), self.offer.clone()) else {
            return;
        };
        self.message = "Downloading update…".into();
        self.phase = Phase::Downloading(0, offer.size());
        self.note = None;
        self.start(worker(move |cancel, send| {
            let client = Client::new(cache).map_err(|e| e.to_string())?;
            client
                .download(&offer, cancel, |progress| {
                    let _ = send.try_send(WorkerEvent::Progress(progress.received, progress.total));
                    makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
                })
                .map(Completed::Downloaded)
                .map_err(|e| e.to_string())
        }));
    }

    fn cancel_running(&mut self) {
        #[cfg(target_os = "macos")]
        restart::cancel();
        if let Some(task) = &self.task {
            task.cancel.cancel();
            self.message = "Cancelling…".into();
            self.note = Some(self.message.clone());
        }
    }

    /// A desktop's input beyond its buttons' own: the channel's pop-up, Try Again
    /// and the keyboard. Choosing a channel checks it at once, as a Mac does.
    /// The keyboard never starts an installation: that takes a real click.
    fn desk_input(&mut self, cx: &mut Cx, event: &Event, actions: &Actions) {
        if self.spin.is_event(event).is_some() {
            self.view.widget(cx, ids!(busy_mark)).redraw(cx);
        }
        let mut changed = false;
        if let Some(item) = self.view.drop_down(cx, ids!(channel)).changed(actions) {
            if item != self.channel && self.task.is_none() && !self.android_install_pending {
                self.channel = item.min(CHANNEL_NAMES.len() - 1);
                self.retire_offer();
                self.dev_release = None;
                self.phase = Phase::Idle;
                self.note = None;
                self.check();
            }
            changed = true;
        }
        let pane = self.pane();
        let retry = |view: &mut Self| match view.phase {
            Phase::Failed(Attempt::Download) => view.download(),
            _ => view.check(),
        };
        if self.view.button(cx, ids!(retry)).clicked(actions) {
            retry(self);
            changed = true;
        }
        if let Event::KeyDown(key) = event {
            let plain = !key.modifiers.any();
            match key.key_code {
                KeyCode::KeyR if key.modifiers.is_primary() && (pane.check || pane.retry) => {
                    retry(self);
                    changed = true;
                }
                KeyCode::Escape if plain && pane.cancel.is_some() => {
                    self.cancel_running();
                    changed = true;
                }
                // Return presses the default button; with none (a plain Check for Updates) it does nothing.
                KeyCode::ReturnKey | KeyCode::NumpadEnter if plain => {
                    let check = pane.check && !pane.check_plain;
                    if pane.link_leads {
                        self.open_notes(cx);
                    } else if check {
                        self.check();
                    } else if pane.download {
                        self.download();
                    } else if pane.retry {
                        retry(self);
                    }
                    changed = check || pane.download || pane.retry;
                }
                _ => {}
            }
        }
        if changed {
            self.refresh(cx);
        }
    }

    /// `tapped`: the click's press and release were both real taps.
    fn install(&mut self, tapped: bool, automatic: bool) {
        if self.task.is_some() || self.android_install_pending {
            return;
        }
        let Some(artifact) = self.artifact.clone() else {
            return;
        };
        // Android's PackageInstaller requires its own human confirmation as well.
        if requires_real_input(automatic) && !tapped {
            self.message = if automatic {
                "Click Install & Restart to continue."
            } else {
                "Tap Install on the phone to review this update."
            }
            .into();
            self.note = Some(self.message.clone());
            return;
        }
        self.message = if automatic { "Preparing the update. OctoSense will ask apps to close before restarting…" }
            else { "Rechecking the downloaded file…" }.into();
        self.phase = Phase::Preparing { restart: automatic };
        self.note = None;
        self.start(worker(move |cancel, _| {
            #[cfg(target_os = "macos")]
            if automatic {
                return octosense_updater::macos::prepare(&artifact, cancel).map(Completed::MacPrepared);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = automatic;
            if cfg!(target_os = "android") {
                let path = artifact.revalidate().map_err(|e| e.to_string())?;
                if cancel.is_cancelled() {
                    return Err("Update cancelled.".into());
                }
                Ok(Completed::AndroidReady(path, artifact.sha256().to_owned()))
            } else {
                install::install(&artifact, cancel).map(Completed::Opened)
            }
        }));
        // Reserve the source while handoff is running: closing the view may
        // race with `open` successfully mounting it before its result arrives.
        // Retaining a verified cache file is safer than deleting a source the
        // operating system is already using.
        if self.task.is_some() {
            self.install_opened = true;
        }
    }

    fn poll(&mut self, cx: &mut Cx) {
        #[cfg(target_os = "macos")]
        if self.restart_waiting && !restart::pending() {
            self.restart_waiting = false;
            self.message = "Restart cancelled. The installed app was not changed.".into();
            self.phase = Phase::Ready;
            self.note = Some(self.message.clone());
            self.refresh(cx);
        }
        let events: Vec<_> = self
            .task
            .as_ref()
            .map(|task| task.receive.try_iter().collect())
            .unwrap_or_default();
        let changed = !events.is_empty();
        for event in events {
            match event {
                WorkerEvent::Progress(received, total) => {
                    self.message = format!(
                        "Downloading: {:.1} / {:.1} MB",
                        received as f64 / 1_000_000.0,
                        total as f64 / 1_000_000.0
                    );
                    self.phase = Phase::Downloading(received, total);
                }
                WorkerEvent::Finished(result) => {
                    let result = *result;
                    let cancelled = self
                        .task
                        .as_ref()
                        .is_some_and(|task| task.cancel.is_cancelled());
                    self.task = None;
                    if cancelled && !matches!(&result, Ok(Completed::Opened(_))) {
                        if let Ok(Completed::Downloaded(artifact)) = result {
                            let _ = artifact.discard();
                        }
                        self.message = "Update cancelled; no installer was opened.".into();
                        self.install_opened = false;
                        // A desktop goes back to where the operation began.
                        (self.phase, self.note) = match self.phase {
                            Phase::Downloading(..) => (Phase::Available, Some("Download cancelled. Nothing was installed.".to_owned())),
                            Phase::Preparing { .. } => (Phase::Ready, Some("Cancelled. Nothing was installed.".to_owned())),
                            _ => (Phase::Idle, None),
                        };
                        continue;
                    }
                    match result {
                        Ok(Completed::Checked(result)) => {
                            self.message = match result.status {
                                CheckStatus::Available => "An update is available. Download it to continue.",
                                CheckStatus::Current => "You are up to date on this channel.",
                                CheckStatus::Development => "This is a development build. Install a release build to enable versioned updates.",
                                CheckStatus::Unavailable => "No compatible release is published on this channel yet.",
                            }.into();
                            self.phase = match result.status {
                                CheckStatus::Available => Phase::Available,
                                CheckStatus::Current => Phase::UpToDate,
                                CheckStatus::Development => Phase::Development,
                                CheckStatus::Unavailable => Phase::NoRelease,
                            };
                            self.note = None;
                            self.last_checked = Some(unix_now());
                            if result.status == CheckStatus::Development {
                                self.dev_release = result.offer.clone();
                            }
                            self.offer = if result.status == CheckStatus::Available {
                                result.offer
                            } else {
                                None
                            };
                        }
                        Ok(Completed::Downloaded(artifact)) => {
                            self.artifact = Some(artifact);
                            self.phase = Phase::Ready;
                            self.note = None;
                            self.message = if cfg!(target_os = "android") {
                                "Download verified. Save your work, then choose Review and install update. Android will ask you to confirm."
                            } else if self.mac_restart {
                                "Download verified. Save your work, then choose Install & Restart. Your profile will be preserved."
                            } else {
                                "Download verified. Save your work, then choose Open verified installer. Installation may restart OctoSense."
                            }.into();
                        }
                        Ok(Completed::Opened(message)) => {
                            self.phase = Phase::Opened;
                            self.note = Some(message.clone());
                            self.message = message;
                            self.install_opened = true;
                        }
                        Ok(Completed::AndroidReady(path, digest)) => {
                            self.android_install_pending = true;
                            self.install_opened = true;
                            self.message =
                                "Waiting for Android to verify and confirm installation…".into();
                            self.android_command(
                                cx,
                                "update_install",
                                serde_json::json!({"path":path,"sha256":digest}),
                            );
                        }
                        #[cfg(target_os = "macos")]
                        Ok(Completed::MacPrepared(prepared)) => {
                            restart::request(prepared);
                            self.restart_waiting = true;
                            self.phase = Phase::RestartWaiting;
                            self.note = None;
                            self.message = "The update is ready. Waiting for your apps to finish saving and close. You can cancel the restart.".into();
                        }
                        Err(error) => {
                            self.install_opened = false;
                            self.failed(&error);
                        }
                    }
                }
            }
        }
        if changed {
            self.refresh(cx);
        }
        let packets = {
            let mut pending = ANDROID.lock().unwrap_or_else(|e| e.into_inner());
            let mut found = Vec::new();
            pending.retain(|value| {
                if value["id"].as_u64() == Some(self.android_request) {
                    found.push(value.clone());
                    false
                } else {
                    true
                }
            });
            found
        };
        for value in packets {
            if let Some(allowed) = value["can_install"].as_bool() {
                self.android_can_install = allowed;
            }
            let status = value["status"].as_str().unwrap_or("error");
            if value["operation"] == "update_info" && status == "ok" {
                self.cache = value["cache_dir"].as_str().map(PathBuf::from);
                self.android_standalone = value["standalone"].as_bool().unwrap_or(false);
                self.android_can_install = value["can_install"].as_bool().unwrap_or(false);
                self.view.label(cx, ids!(current)).set_text(
                    cx,
                    &format!(
                        "Installed: OctoSense Home {}",
                        value["version_name"].as_str().unwrap_or("unknown")
                    ),
                );
                self.message = if let Some(reason) =
                    value["message"].as_str().filter(|text| !text.is_empty())
                {
                    reason.chars().take(1024).collect()
                } else if self.android_standalone {
                    "Check for an update when you are ready.".into()
                } else {
                    "This installation uses a different signing identity. Use Settings → Updates for ROM updates, or obtain a compatible build from your distributor. Your installed Home and data will be kept.".into()
                };
                if self.android_standalone {
                    // An OS installation can outlive this window or replace the
                    // process. Recover its durable status before offering work.
                    self.android_install_pending = true;
                    self.android_command(cx, "update_status", serde_json::json!({}));
                }
            } else {
                self.message = value["message"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(if status == "idle" {
                        "Check for an update when you are ready."
                    } else {
                        status
                    })
                    .chars()
                    .take(1024)
                    .collect();
                self.android_install_pending =
                    matches!(status, "preparing" | "awaiting_confirmation");
                if status == "permission_required" {
                    self.android_can_install = false;
                }
            }
            self.refresh(cx);
        }
    }
}

impl Widget for UpdaterView {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.initialize(cx);
        self.poll(cx);
        if self.pointer && self.pane().spinner {
            self.spin = cx.new_next_frame();
        }
        let step = self.view.draw_walk(cx, scope, walk);
        self.fit_window(cx);
        step
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.initialize(cx);
        self.poll(cx);
        // Read this view's clicks while the input that made them is still
        // being handled: only then does `trusted_user_input` report a real
        // tap, which Android's install and permission steps require, as in
        // the host's other approval sheets (`connected_review.rs`). A click
        // read later from `Event::Actions` is never a trusted one.
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        let trusted = makepad_widgets::makepad_platform::trusted_user_input();
        if self.view.button(cx, ids!(install)).pressed(&actions) {
            self.install_down = trusted;
        }
        if self.view.button(cx, ids!(permission)).pressed(&actions) {
            self.permission_down = trusted;
        }
        if self.pointer {
            self.desk_input(cx, event, &actions);
        }
        let changed = [
            ids!(channel),
            ids!(check),
            ids!(check_plain),
            ids!(check_more),
            ids!(download),
            ids!(cancel),
            ids!(install),
            ids!(manual),
            ids!(notes),
            ids!(notes_plain),
            ids!(notes_lead),
            ids!(permission),
            ids!(retry_install),
        ]
        .iter()
        .any(|id| self.view.button(cx, *id).clicked(&actions));
        if !changed {
            return;
        }
        if !self.pointer
            && self.view.button(cx, ids!(channel)).clicked(&actions)
            && self.task.is_none()
            && !self.android_install_pending
        {
            self.channel = (self.channel + 1) % 3;
            self.retire_offer();
            self.message = "Channel changed. Check for updates to continue.".into();
        }
        if [ids!(check), ids!(check_plain), ids!(check_more)].iter().any(|id| self.view.button(cx, *id).clicked(&actions)) {
            self.check();
        }
        if self.view.button(cx, ids!(download)).clicked(&actions) {
            self.download();
        }
        if self.view.button(cx, ids!(cancel)).clicked(&actions) {
            self.cancel_running();
        }
        if self.view.button(cx, ids!(install)).clicked(&actions) {
            let down = std::mem::take(&mut self.install_down);
            self.install(down && trusted, self.mac_restart);
        }
        if self.view.button(cx, ids!(manual)).clicked(&actions) {
            self.install(false, false);
        }
        if [ids!(notes), ids!(notes_plain), ids!(notes_lead)].iter().any(|id| self.view.button(cx, *id).clicked(&actions)) {
            self.open_notes(cx);
        }
        if self.view.button(cx, ids!(permission)).clicked(&actions)
            && std::mem::take(&mut self.permission_down)
            && trusted
        {
            self.android_command(cx, "update_allow_install", serde_json::json!({}));
        }
        if self.view.button(cx, ids!(retry_install)).clicked(&actions) {
            self.android_command(cx, "update_status", serde_json::json!({}));
        }
        self.refresh(cx);
    }
}

pub struct UpdaterModule;
pub static UPDATER_MODULE: UpdaterModule = UpdaterModule;
impl AppModule for UpdaterModule {
    fn id(&self) -> &'static str {
        "updater"
    }
    fn label(&self) -> &'static str {
        "OctoSense Updates"
    }
    fn register(&self, vm: &mut ScriptVm) {
        script_mod(vm);
    }
    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &[]
    }
    fn create(&self, vm: &mut ScriptVm, _: ValidatedOpen, handles: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, { use mod.widgets.* UpdaterView {} });
        let root = WidgetRef::script_from_value(vm, value);
        if let Some(mut view) = root.borrow_mut::<UpdaterView>() {
            view.windows = Some(handles.windows);
        }
        let closing = root.clone();
        InstanceParts {
            root,
            executor: Box::new(NoTools),
            shutdown: Box::new(move |vm| {
                if let Some(mut view) = closing.borrow_mut::<UpdaterView>() {
                    #[cfg(target_os = "macos")]
                    if view.restart_waiting { restart::cancel(); }
                    if let Some(task) = view.task.take() {
                        task.cancel.cancel();
                    }
                    view.retire_offer();
                    if cfg!(target_os = "android") {
                        // Cancel queued Settings openings as well as APK prep.
                        // A session already handed to Android is preserved.
                        view.android_command(vm.cx_mut(), "update_cancel", serde_json::json!({}));
                    }
                }
            }),
        }
    }
}
struct NoTools;
impl ServiceExecutor for NoTools {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new(
            "updater",
            "OctoSense Updates",
            "Install reviewed OctoSense releases. No agent tools.",
        )
    }
    fn execute(&mut self, _: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(
            &call.call_id,
            "Updates can only be requested in the host-owned Updates window",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_and_restart_acts_only_on_a_real_click() {
        assert!(super::requires_real_input(true));
        assert_eq!(super::requires_real_input(false), cfg!(target_os = "android"));
    }

    fn facts(phase: Phase) -> Facts<'static> {
        Facts {
            phase,
            supported: true,
            installed: Some("0.1.0 RC 5".into()),
            newest: Some("0.1.0 RC 6".into()),
            size: 123_400_000,
            channel: "Release Candidate",
            restart: true,
            note: None,
            mac: false,
        }
    }

    #[test]
    fn a_macs_up_to_date_words_end_on_more_than_one_word() {
        let mac = pane_words(&Facts { mac: true, ..facts(Phase::UpToDate) });
        assert_eq!(mac.status, "0.1.0 RC 5 is the newest Release Candidate version.");
        let other = pane_words(&facts(Phase::UpToDate));
        assert_eq!(other.status, "0.1.0 RC 5 is the newest version on the Release Candidate channel.");
    }

    const PHASES: [Phase; 16] = [
        Phase::Idle,
        Phase::Checking,
        Phase::UpToDate,
        Phase::Development,
        Phase::NoRelease,
        Phase::Available,
        Phase::Downloading(0, 0),
        Phase::Downloading(61_700_000, 123_400_000),
        Phase::Ready,
        Phase::Preparing { restart: true },
        Phase::Preparing { restart: false },
        Phase::RestartWaiting,
        Phase::Opened,
        Phase::Failed(Attempt::Check),
        Phase::Failed(Attempt::Download),
        Phase::Failed(Attempt::Install),
    ];

    #[test]
    fn a_pane_offers_one_default_at_most_and_never_tells_a_person_to_go_elsewhere() {
        for phase in PHASES {
            let pane = pane_words(&facts(phase));
            assert!(!pane.headline.is_empty(), "{phase:?}");
            let defaults = [pane.check && !pane.link_leads && !pane.check_plain, pane.download, pane.retry, pane.install.is_some(), pane.link_leads];
            assert!(defaults.iter().filter(|on| **on).count() <= 1, "{phase:?}");
            let running = matches!(
                phase,
                Phase::Checking | Phase::Downloading(..) | Phase::Preparing { .. } | Phase::RestartWaiting
            );
            // Under way: it can be cancelled. Otherwise there is something to do next,
            // or nothing waits and Check for Updates is there, plain.
            assert_eq!(pane.cancel.is_some(), running, "{phase:?}");
            assert!(running || defaults.iter().any(|on| *on) || (pane.check && pane.check_plain), "{phase:?}");
            let text = format!("{} {}", pane.headline, pane.status);
            for word in ["GitHub", "metadata", "SHA", "asset", "tag", "semantic", "tap ", "Tap "] {
                assert!(!text.contains(word), "{phase:?} says {word}: {text}");
            }
        }
    }

    #[test]
    fn a_development_build_leads_with_the_way_to_a_release() {
        // Its copy says to install a release, so that step is the default, as
        // Software Update's step that moves you on is.
        let pane = pane_words(&facts(Phase::Development));
        assert_eq!((pane.link, pane.link_leads, pane.check), (Some("Get Release Build…"), true, true));
        for phase in PHASES.iter().filter(|p| **p != Phase::Development) {
            assert!(!pane_words(&facts(*phase)).link_leads, "{phase:?}");
        }
    }

    #[test]
    fn check_for_updates_is_plain_when_nothing_waits() {
        // Up to date, an installer opened, or no release here (the next step is the
        // channel pop-up): a plain Check for Updates and no default, as Software Update.
        for phase in [Phase::UpToDate, Phase::NoRelease, Phase::Opened] {
            let pane = pane_words(&facts(phase));
            assert!(pane.check && pane.check_plain, "{phase:?}");
        }
        // Never checked yet: checking is the step, the default.
        let first = pane_words(&facts(Phase::Idle));
        assert!(first.check && !first.check_plain);
    }

    #[test]
    fn a_failure_can_be_tried_again() {
        for attempt in [Attempt::Check, Attempt::Download] {
            assert!(pane_words(&facts(Phase::Failed(attempt))).retry);
        }
        // Through the same button as Install & Restart, which takes a real click.
        let install = pane_words(&facts(Phase::Failed(Attempt::Install)));
        assert_eq!(install.install, Some("Try Again"));
        assert!(!install.retry);
    }

    #[test]
    fn a_computer_that_cant_update_says_so_and_offers_no_check() {
        let pane = pane_words(&Facts { supported: false, ..facts(Phase::Idle) });
        assert!(!pane.check && !pane.checked);
        assert_eq!(pane.link, Some("View Releases"));
    }

    #[test]
    fn a_release_build_ignores_the_test_hook() {
        let release = BuildIdentity::from_tag(Product::Desktop, Some("desktop-v0.1.0-rc.6")).unwrap();
        for hook in ["desktop-v0.1.0-rc.5", "desktop-v0.1.0-rc.5+restart", "unsupported"] {
            let (identity, unsupported, restart) = test_identity(release.clone(), Some(hook));
            assert_eq!(identity.tag(), Some("desktop-v0.1.0-rc.6"), "{hook}");
            assert!(!unsupported && !restart, "{hook}");
        }
        let development = BuildIdentity::from_tag(Product::Desktop, None).unwrap();
        let (identity, unsupported, restart) =
            test_identity(development.clone(), Some("desktop-v0.1.0-rc.5+restart"));
        assert_eq!(identity.tag(), Some("desktop-v0.1.0-rc.5"));
        assert!(!unsupported && restart);
        assert!(test_identity(development.clone(), Some("unsupported")).1);
        // A value that is no release leaves the build as it is.
        assert_eq!(test_identity(development, Some("not-a-tag")).0.tag(), None);
    }

    #[test]
    fn failures_read_as_sentences_a_person_can_act_on() {
        use octosense_updater::Error;
        let errors = [
            Error::Network,
            Error::Timeout,
            Error::RateLimited,
            Error::InvalidMetadata("ambiguous release version"),
            Error::Integrity("digest mismatch"),
            Error::Storage("disk full"),
            Error::Unsupported,
        ];
        for error in errors {
            let raw = error.to_string();
            let words = plain_failure(&raw);
            assert_ne!(words, raw);
            for word in ["GitHub", "metadata", "digest", "SHA"] {
                assert!(!words.contains(word), "{words}");
            }
        }
        assert_eq!(plain_failure("It broke."), "It broke.");
        assert_eq!(plain_failure("Use Open installer manually."), "Use Open Installer Manually.");
    }

    #[test]
    fn system_failures_do_not_expose_os_errors_or_exit_codes() {
        for error in [
            "Could not start update worker: Resource temporarily unavailable (os error 35)",
            "Could not open the verified installer: No such file or directory (os error 2)",
            "Could not stage the new app (exit status: 1). Use Open installer instead.",
            "Could not close the disk image (signal: 9). Use Open installer instead.",
            "Permission denied (os error 13)",
        ] {
            assert_eq!(plain_failure(error), "OctoSense couldn’t complete the update request. Try again.");
        }
    }

    #[test]
    fn versions_read_as_a_person_says_them() {
        assert_eq!(plain_version("0.1.0-rc.6"), "0.1.0 RC 6");
        assert_eq!(plain_version("0.1.0-beta.2"), "0.1.0 Beta 2");
        assert_eq!(plain_version("1.2.0"), "1.2.0");
    }

    #[test]
    fn last_checked_reads_like_a_mac() {
        // Nov 2, 2026, 15:45 UTC.
        let now = 1_793_634_300;
        assert_eq!(last_checked_words(None, now, 0), "Last checked: Never");
        assert_eq!(last_checked_words(Some(now), now, 0), "Last checked: Today at 3:45 PM");
        assert_eq!(last_checked_words(Some(now - 86_400), now, 0), "Last checked: Yesterday at 3:45 PM");
        assert_eq!(last_checked_words(Some(now - 15 * 3_600 - 45 * 60), now, 0), "Last checked: Today at 12:00 AM");
        assert_eq!(last_checked_words(Some(now - 2 * 86_400), now, 0), "Last checked: Oct 31, 2026 at 3:45 PM");
        // The same moment seven hours behind UTC: the morning of that day.
        assert_eq!(last_checked_words(Some(now), now, -7 * 3_600), "Last checked: Today at 8:45 AM");
        // Twelve hours ahead it is already Nov 3 there, and 11:45 UTC was late on Nov 2.
        assert_eq!(last_checked_words(Some(now - 4 * 3_600), now, 12 * 3_600), "Last checked: Yesterday at 11:45 PM");
    }
}
