//! octosense-shell: the one OctoSense shell — window manager, hosting, phone
//! layer — linked by both packages (desktop/: `octosense`, phone/:
//! `octosense-home`). A package owns only its entry point (`octosense_main!`)
//! and what is product-specific (the phone's Settings app, through `ext`).
//!
//! Chrome is styled entirely in splash: the theme is a `theme.splash` file
//! (imported from omarchy themes, see theme.rs) evaluated into the VM
//! before the app module, so the DSL below reads `mod.wm_theme.*`.

#![allow(dead_code)] // shell surface (icons, OSD, panels) built ahead of the flows that use it

pub mod agents;
#[cfg(any(feature = "app-hub", native_mobile))]
pub mod agent_events;
pub mod ai_bus;
pub mod app_chat;
pub mod app_storage;
pub mod dev_mode;
pub mod approvals;
pub mod apps;
pub mod binds;
pub mod clients;
pub mod demo_home;
pub mod desk;
pub mod desktop_layout;
pub mod desktop;
pub mod desktop_app;
pub mod snap;
pub mod mobile;
pub mod mobile_navigation;
pub mod mobile_surface;
pub mod mobile_gestures;
pub mod mobile_hints;
pub mod mobile_app;
pub mod mobile_back;
pub mod mobile_tiles;
pub mod mobile_shade;
pub mod mobile_pages;
pub mod mobile_island;
pub mod mobile_octopus;
pub mod mobile_groups;
pub mod mobile_perf;
pub mod mobile_theme;
pub mod android_integration;
pub mod android_accessibility;
pub mod scene;
pub mod dock_warp;
pub mod host;
pub mod host_tools;
pub mod hub;
pub mod layout;
pub mod octosense;
pub mod module_host;
#[cfg(test)]
mod module_close_tests;
#[cfg(test)]
mod module_input_tests;
#[cfg(test)]
mod module_panic_tests;
#[cfg(test)]
mod module_peer_tests;
#[cfg(test)]
mod module_resize_tests;
#[cfg(test)]
mod system_app_theme_tests;
pub mod module_view;
pub mod native_apps;
pub mod sandbox;
pub mod pane_links;
pub mod peer_link;
pub mod preview;
pub mod process_close;
pub mod questions;
pub mod run_view;
pub mod shell;
pub mod theme;
pub mod tile;
pub mod wm_reply;
pub mod ext;
pub mod glance;
pub mod glance_card;
pub mod glance_chat;
pub mod mail_card;
#[cfg(any(feature = "app-hub", native_mobile))]
pub mod mail_review;
pub mod glance_digest;
#[cfg(any(feature = "app-hub", native_mobile))]
pub mod glance_notice;
pub mod glance_panel;
pub mod glance_sheet;
mod card_chat;
use glance::NoteTargets as GlanceNoteTargets;
// The App derive takes a plain type name for a field.
use approvals::RequestNotices as ApprovalNotices;
pub mod system_chat;
pub use octosense_ai_host as ai_host;

// A package's `main.rs` is `octosense_main!()` (below); what only it does
// (the phone's Settings app) wraps this `App` and plugs in through `ext`.

pub use makepad_widgets;
use makepad_widgets::makepad_platform::thread::{Lane, SignalToUI, TaskHandle};
use makepad_widgets::*;

/// The standalone mobile shell: the Android phone shell fills the window
/// and nothing else is offered — no desk bar, no style switcher, no
/// desktop styles. build.rs sets the `mobile_only` cfg for Android builds
/// and for `--features mobile-only`; this is its one Rust-side name.
pub const MOBILE_ONLY: bool = cfg!(mobile_only);

use std::collections::HashMap;

use binds::{match_bind_armed, WmAction};
use desk::{WmDesk, WmDeskAction, WmState};
use clients::{spawn_client, ClientLine, LaunchPolicy, WarmPool, WarmStatus};
use makepad_widgets::makepad_platform::shared_framebuf::{
    shared_swapchain_from_host_swapchain, HostSwapchain,
};
use hub::{send_to_app, ClientId, HubEvent, WmHub};
use layout::{Axis, Dir, DividerHit, FullscreenMode, LRect};
use makepad_studio_protocol::{AppToStudio, StudioToApp};
use makepad_wm_api::{WmEvent, WmRequest};
use preview::PreviewCache;
use mobile_gestures::GestureRecognizer;
use run_view::{MpRunView, MpRunViewAction};
use makepad_widgets::makepad_micro_serde::*;
use shell::bar::{BarData, BarModule, SampledStatus, ShellBarAction};
use shell::menu::{MenuSkin, ShellMenu, ShellMenuAction};
use shell::panels::ShellPanelAction;
use ai_bus::{AiBus, Route};
use apps::{AppRegistry, Hosting};
use android_integration::AndroidRuntime;
use makepad_ai_services::wire::{ServiceCall, ServiceDown, ToolResult};
use makepad_app_module::{AppModule, CloseDecision, ExecOutcome, ModuleCloseAction, ModuleUpstream, WindowRequest};
use module_host::ModuleHost;
use process_close::ProcessCloseGate;
use pane_links::{PaneCall, PaneLinks};
use makepad_widgets::ai_slot::AiSlotRequests;
use shell::ai_pane::ShellAiPane;


script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // A tray glyph: one of our own SVGs on a single quad. The icons are
    // drawn at their natural size in one 16-unit box, so `scale` keeps
    // their proportions honest against each other (a battery IS wider and
    // shorter than a speaker) at roughly the reference tray's 9-10px.
    let TrayIcon = Icon{
        width: Fit
        height: Fill
        align: Align{x: 0.5 y: 0.5}
        icon_walk: Walk{width: Fit height: Fit}
        draw_icon +: {
            scale: 0.9
            color: mod.wm_theme.foreground
        }
    }

    // The shell's root UI, by name: a package whose `App` wraps the shell's
    // (the phone, for Settings) seats the same tree in its own `startup`.
    mod.widgets.OctoSenseRoot = Root{
            main_window := Window{
                window.inner_size: vec2(1400, 900)
                window.title: "OctoSense"
                // No native caption: the Omarchy bar IS the caption. Our
                // WindowDragQuery answers Caption over the bar strip (minus
                // its buttons) so the window still drags.
                show_caption_bar: false
                body +: {
                    flow: Down
                    // Keep hosted editors and their navigation visible above
                    // the native IME instead of panning the whole shell.
                    keyboard_resize: #(crate::mobile_navigation::ENABLED)
                    // The wallpaper layer: the theme's image (crop-to-fill)
                    // over the theme's deep background.
                        // The desk bar: the shell bar on a desktop style, the
                        // phone's style/appearance/rotate strip on a phone
                        // style. The standalone shell has neither.
                        bar := SolidView{
                            width: Fill
                            height: 26
                            visible: #(!MOBILE_ONLY)
                            flow: Overlay
                            draw_bg +: {
                                color: mod.wm_theme.background
                            }
                            desktop_controls := View{
                                width: Fill height: Fill
                                shell_bar := ShellBar{width: Fill height: Fill}
                            }
                            phone_controls := PhoneSurface{visible: false}
                        }
                    scene := WmScene{
                    wallpaper := CachedView{width: Fill height: Fill flow: Overlay
                    bg_fill := RectView{
                        width: Fill
                        height: Fill
                        draw_bg +: {
                            color_top: uniform(mod.wm_theme.darker_background)
                            color_bottom: uniform(mod.wm_theme.background)
                            pixel: fn() {
                                return mix(self.color_top, self.color_bottom, self.pos.y)
                            }
                        }
                    }
                    bg_image := Image{
                        width: Fill
                        height: Fill
                        fit: ImageFit.CropToFill
                        visible: false
                    }
                    }
                        desk_row := View{
                            width: Fill
                            height: Fill
                            flow: Right
                            shell_ai_pane := ShellAiPane{}
                            desk := WmDesk{
                                Tile := MpRunView{}
                                // An in-process module instance's tile (module_view.rs).
                                ModuleTile := MpModuleView{}
                            }
                        }
                    // The shell's floating surfaces, over the desk: the
                    // menu/launcher, the bar flyouts, the OSD and the
                    // notification stack. Each draws only when it is up.
                    shell_overlay := View{
                        width: Fill
                        height: Fill
                        flow: Overlay
                        desktop_shelf := DesktopShelf{}
                        snap_overlay := SnapOverlay{}
                        // Published glance cards (glance_panel.rs), F9.
                        shell_glance := ShellGlancePanel{}
                        shell_panel := ShellPanel{}
                        shell_menu := ShellMenu{}
                        // The approval surface (approvals/): the shell's
                        // approval and first-use sheets, the time-box
                        // indicator, and Settings > Assistant > Approvals.
                        // The system chat (system_chat/): the person's
                        // conversation with the system agent, F8. Under the
                        // approval sheets, which answer its approvals.
                        shell_system_chat := ShellSystemChat{}
                        // "Ask <app>" (app_chat/): an app agent's
                        // conversation, both lanes, beside the system chat.
                        shell_app_chat := ShellSystemChat{ app_panel: true }
                        // Cards and notifications stay above phone chat;
                        // approval sheets and the developer banner remain above them.
                        shell_glance_sheet := ShellGlanceSheet{}
                        shell_notes := ShellNotifications{}
                        shell_osd := ShellOsd{}
                        shell_approvals := ShellApprovals{}
                        shell_approvals_settings := ShellApprovalsSettings{}
                        // Developer mode's banner (dev_mode.rs): over
                        // everything, drawn only while the mode is on.
                        shell_dev_banner := ShellDevBanner{}
                    }
                    }
                    // `wm --gallery`: every ported omarchy surface with
                    // fixture data, over the desktop (see shell/gallery.rs).
                    // `wm --gallery` fills this at startup (handle_startup):
                    // the gallery is a deep fixture tree nobody else pays for
                    // — on the web its construction alone overflows the 1 MiB
                    // wasm stack.
                    gallery_holder := View{
                        width: Fill
                        height: Fill
                        visible: false
                        shell_gallery_host := ShellGalleryHost{}
                    }
                }
            }
        }

    startup() do #(App::script_component(vm)){
        ui: mod.widgets.OctoSenseRoot{}
    }
}

// ======================================================================
// App
// ======================================================================

/// Bar height when the platform reports no window-button rect (Linux,
/// Windows, and macOS before the first geometry event).
const BAR_HEIGHT_FALLBACK: f64 = 26.0;
/// `developer_options_activate`'s target for the phone's finished gesture.
const DEVELOPER_PHONE_ON: &str = "setup.developer.phone-on";

/// The bar's height and the left padding its content starts at. macOS puts
/// its traffic lights on the left; Linux and Windows put caption buttons on
/// the right, where they must not push the left cluster off screen.
#[cfg(not(mobile_only))]
type BarMetrics = (f64, f64);

#[cfg(not(mobile_only))]
fn bar_metrics_for_geom(geom: &WindowGeom, native_mobile: bool) -> BarMetrics {
    if native_mobile {
        return (48.0, 8.0);
    }
    let buttons = geom.window_chrome_buttons;
    if buttons.size.y <= 0.0 {
        return (BAR_HEIGHT_FALLBACK, 84.0);
    }
    let height = (buttons.pos.y * 2.0 + buttons.size.y).ceil();
    let buttons_are_on_left = buttons.pos.x + buttons.size.x <= geom.inner_size.x * 0.5;
    let pad_left = if buttons_are_on_left {
        buttons.pos.x + buttons.size.x + 12.0
    } else {
        8.0
    };
    (height, pad_left)
}

#[cfg(all(test, not(mobile_only)))]
mod bar_chrome_tests {
    use super::*;

    #[test]
    fn left_caption_buttons_move_the_left_cluster_past_them() {
        let geom = WindowGeom {
            inner_size: dvec2(1000.0, 700.0),
            window_chrome_buttons: Rect {
                pos: dvec2(12.0, 9.0),
                size: dvec2(72.0, 24.0),
            },
            ..Default::default()
        };
        assert_eq!(bar_metrics_for_geom(&geom, false), (42.0, 96.0));
    }

    #[test]
    fn right_caption_buttons_do_not_push_the_left_cluster_off_screen() {
        let geom = WindowGeom {
            inner_size: dvec2(917.0, 1030.0),
            window_chrome_buttons: Rect {
                pos: dvec2(779.0, 0.0),
                size: dvec2(138.0, 29.0),
            },
            ..Default::default()
        };
        assert_eq!(bar_metrics_for_geom(&geom, false), (29.0, 8.0));
    }

    #[test]
    fn native_mobile_toolbar_has_touch_height_without_caption_space() {
        let geom = WindowGeom::default();
        assert_eq!(bar_metrics_for_geom(&geom, true), (48.0, 8.0));
        assert_eq!(bar_metrics_for_geom(&geom, false), (26.0, 84.0));
    }
}

/// The octos reader's WebView renderer host (feature `toolbox-peers`;
/// nothing without it). A plain field type: the `Script` derive takes no
/// `#[cfg]` on fields.
#[derive(Default)]
pub struct WebViewRender {
    #[cfg(feature = "toolbox-peers")]
    host: octosense_ai_host::webview_render::WebViewRenderHost,
}

impl WebViewRender {
    #[allow(unused_variables)]
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        #[cfg(feature = "toolbox-peers")]
        self.host.handle_event(cx, event);
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    pub ui: WidgetRef,
    #[rust]
    pub state: Option<WmState>,
    #[rust]
    pub android_runtime: AndroidRuntime,
    /// Module instances launched this session: a package that watches
    /// launches (the phone's Settings) compares it after each event.
    #[rust]
    pub modules_launched: u64,
    #[rust]
    pub snap_hover_timer: Timer,
    /// A keyboard focus that could not land yet (the tile hadn't drawn);
    /// re-asserted when that client's first frame arrives.
    #[rust]
    pub pending_focus: Option<ClientId>,
    /// The client whose tile last took the keyboard. It lets go before
    /// another tile takes it: a module tile never moves key focus itself
    /// (its widgets claim it), so without this the hidden tab of a group
    /// kept holding the keyboard and every key was dropped.
    #[rust]
    pub keyboard_holder: Option<ClientId>,
    /// `--gallery`: the shell-surface gallery instead of a desktop.
    #[rust]
    pub gallery: bool,
    /// What the shell bar's status modules show, sampled on the tick.
    #[rust]
    pub bar_sample: BarData,
    #[rust]
    pub status_rx: Option<std::sync::mpsc::Receiver<SampledStatus>>,
    #[rust]
    pub status_worker: Option<TaskHandle<()>>,
    #[rust]
    pub status_sample: SampledStatus,
    #[rust]
    pub status_tick: u32,
    /// The clock's alternate format (right-click cycles it, `formatAlt`).
    #[rust]
    pub clock_alt: bool,
    /// The theme's wallpapers are being fetched (a fresh install ships the
    /// colours, not the pictures); the tick applies the first one to land.
    #[rust]
    pub backgrounds_pending: bool,
    #[rust]
    pub background_task: Option<TaskHandle<usize>>,
    /// Which bar module's flyout is open, for the accent pill.
    #[rust]
    pub shell_panel_open: Option<BarModule>,
    /// The glance cards' generation the surfaces last drew (glance.rs).
    #[rust]
    pub glance_generation: u64,
    /// When the glance panel was last open: cards published after it are
    /// new (the bar's glance button is lit).
    #[rust]
    pub glance_seen_ms: u64,
    /// A quit waiting on apps asking the person: since when (its start or
    /// the last refusal), so it expires (process_close `quit_expired`).
    #[rust]
    pub quit_waiting_since: Option<f64>,
    /// The notifications cards asked for (`glance.publish` with `notify`):
    /// the desktop toasts' ids and the card each opens, and the phone
    /// shade's ids and the exact card each opens.
    #[rust]
    pub glance_toasts: GlanceNoteTargets,
    /// The toast offering to undo the last dismissal in the glance panel.
    #[rust]
    pub glance_undo_toast: Option<u64>,
    #[rust]
    pub glance_shade_notes: GlanceNoteTargets,
    /// The developer-mode generation last acted on (dev_mode.rs).
    #[rust]
    pub dev_generation: u64,
    /// The phone's developer-options gesture, finished in this run: About
    /// phone shows Developer options (`developer_build_tap`).
    #[rust]
    developer_options_revealed: Option<dev_mode::TapsReached>,
    /// The approvals' generation last drawn (approvals/).
    #[rust]
    pub approvals_generation: u64,
    /// The notifications of "Needs you" notices, withdrawn with their
    /// requests' sheet lines (approvals/).
    #[rust]
    pub approval_notices: ApprovalNotices,
    /// The system chat's generation the surfaces last drew (system_chat/).
    #[rust]
    pub system_chat_generation: u64,
    #[rust]
    pub hub: Option<WmHub>,
    #[rust]
    pub next_id: ClientId,
    #[rust]
    pub tick: Timer,
    /// Bar height + left padding last applied from the OS window buttons.
    #[rust]
    #[cfg(not(mobile_only))]
    pub bar_metrics: Option<BarMetrics>,
    /// SUPER+mouse:272/273 move & resize (tiling.lua).
    #[rust]
    pub drag: Option<DragState>,
    /// A divider drag in flight: a plain press IN THE GAP between two
    /// tiles moves that split (see `DividerDrag`).
    #[rust]
    pub div_drag: Option<DividerDrag>,
    /// The axis of the divider band the pointer is hovering, so the resize
    /// cursor is set once on the way in and cleared once on the way out.
    #[rust]
    pub div_hover: Option<Axis>,
    /// The one-shot SUPER+ALT layer prefix (see binds.rs).
    #[rust]
    pub alt_armed: bool,
    /// The bar was hidden because a window went fullscreen, not by
    /// SUPER+SHIFT+SPACE — so it comes back on its own.
    #[rust]
    pub bar_hidden_by_fullscreen: bool,
    /// Output lines from every child, for the tile's status line.
    #[rust]
    pub client_lines: Option<ClientLines>,
    /// The workspaces the bar cluster is currently showing, left to right —
    /// what a click on it maps to.
    #[rust]
    pub bar_workspaces: Vec<usize>,
    /// Quick Look's warm-viewer cache (see `preview::PreviewCache`).
    #[rust]
    pub preview_cache: PreviewCache,
    /// The warm-instance pool: which standby processes exist, per app.
    #[rust]
    pub warm_pool: WarmPool,
    /// The off-desk framebuffer each warm instance draws into before it has
    /// a tile (see `WarmFrame`).
    #[rust]
    pub warm_frames: HashMap<ClientId, WarmFrame>,
    /// Drives the dormant instances (see `pump_warm`).
    #[rust]
    pub warm_tick: Timer,
    /// `--test-action ask-appcard:<text>`: the text and the timer that
    /// submits it to the appcard instance's `ask` tool once the hosted
    /// app has had time to bring its kernel and sessions up.
    #[rust]
    pub test_asks: Vec<(Timer, String)>,
    /// `--test-action capture:<path>`: every few seconds the next presented
    /// frame is written to <path> as PNG, so a scripted run can be looked
    /// at without a screen (the GPU readback does not need one).
    #[rust]
    pub test_capture: Option<(Timer, std::path::PathBuf)>,
    #[rust] pub test_capture_ticket: Option<ReadbackTicket>,
    #[rust] pub test_recording: bool,
    /// `--test-action page:<n>`: the home page to jump to, once the phone
    /// home has laid its pages out at the phone's size (mobile_pages.rs).
    #[rust]
    pub test_page: Option<(Timer, i64)>,
    /// The hidden WebView that renders pages for the octos reader where
    /// there is no Chrome (the phone): serves `webview_render::renderer`.
    #[rust]
    pub webview_render: WebViewRender,
    /// `--test-action taps:<x>,<y>@<s>[;…]`: touch for a run that has none
    /// (the headless iOS simulator first of all). Each entry puts a finger
    /// down at window point (x, y) <s> seconds after startup and lifts it
    /// a beat later, through the same event path as a real touch.
    #[rust]
    pub test_taps: Vec<(Timer, Vec2d, makepad_platform::event::TouchState)>,
    /// When each warm client was last ticked. Kept apart from `WarmFrame`
    /// because the FIRST ticks are what make a frame possible at all — see
    /// `pump_warm`.
    #[rust]
    pub warm_last_tick: HashMap<ClientId, f64>,
    /// The desktop window's dpi factor, as the platform last reported it —
    /// a warm instance is configured with the same one its tile will use,
    /// so the glyph atlas and shaders it warms up are the right ones.
    #[rust]
    pub dpi_factor: f64,
    /// The AI services bus: which client is the pane, every client's last
    /// registration, the routing (see ai_bus.rs).
    #[rust]
    pub ai_bus: AiBus,
    /// Which of the theme's wallpapers is up (SUPER+CTRL+SPACE cycles).
    #[rust]
    pub background_index: usize,
    /// The instances this process hosts itself (module_host.rs): one
    /// isolate each, seated in a `ModuleTile`.
    #[rust]
    pub module_host: ModuleHost,
    /// Process apps that answer a close themselves, and the closes they
    /// are answering (process_close.rs, OctoSense#179).
    #[rust]
    pub process_close: ProcessCloseGate,
    /// Extra windows module instances opened (`ModuleWindows`): window
    /// client -> (owner instance's client, the instance's key for it).
    #[rust]
    pub module_windows: HashMap<ClientId, (ClientId, LiveId)>,
    /// The in-process transport to an assistant seated in the pane as a
    /// module: the WM's services as links its registry adopts (pane_links.rs).
    #[rust]
    pub pane_links: PaneLinks,
    /// Which apps are linked as modules and which the person switched to
    /// module hosting (apps.rs).
    #[rust]
    pub apps: AppRegistry,
    #[rust] pub style_frame: NextFrame,
    #[rust] pub style_time: f64,
    #[rust] pub title_press: Option<(ClientId, f64, Vec2d)>,
    #[rust] pub stylesheet: Option<desktop_style::StyleSheet>,
    #[rust] pub phone_frame: NextFrame,
    #[rust] pub phone_time: f64,
    /// The status-bar clock the phone last drew: `phone_tick` redraws when
    /// the minute changes (mobile_app.rs).
    #[rust] pub phone_clock_shown: Option<String>,
    /// The shell gesture recognizer (mobile_gestures.rs): the one owner of
    /// the finger the phone shell claims.
    #[rust] pub phone_gestures: GestureRecognizer,
    /// Frames a Commit/Cancel has been visible in `phone.gesture_out`: the
    /// surfaces get one drawn frame to see it before it clears.
    #[rust] pub gesture_out_age: u32,
}

/// A warm instance's own swapchain: the host end of the frames a DORMANT
/// client draws while it has no tile.
///
/// This is what makes the pool worth more than a running process. Handed a
/// geometry and a framebuffer, the child does its FIRST full draw pass
/// while nobody is waiting — compiling its shaders, rasterizing its glyph
/// atlas, laying out its UI — so adoption costs one geometry message and a
/// repaint by an app that is already warm all the way to the GPU.
pub struct WarmFrame {
    /// `None` on Linux, where sharing a framebuffer needs the aux-chan
    /// socket a TILE owns; a warm instance there is process-warm only and
    /// gets its framebuffer at adoption.
    swapchain: Option<HostSwapchain>,
    /// The instance drew at least once: it is warm all the way through,
    /// and drops to the heartbeat tick rate.
    presented: bool,
    /// Set at adoption. The tile has its own swapchain from that moment,
    /// but the child may still have a frame in flight against this one, so
    /// it is held briefly before being dropped — the same hazard
    /// `MpRunView::last_swapchain_with_completed_draws` covers on resize.
    retired: Option<f64>,
}

/// How long a retired warm swapchain is kept after adoption.
const WARM_FRAME_RETIRE: f64 = 2.0;

/// The pump interval: fast enough that a warm instance reaches its first
/// frame promptly, slow enough to be nothing on the WM's own clock.
const WARM_PUMP: f64 = 0.05;

/// What a DORMANT instance gets once it has drawn: enough to keep its
/// watchdog and any app-side timer alive, little enough to be free.
const WARM_HEARTBEAT: f64 = 1.0;

/// The channel every client's reader thread writes its output into.
pub struct ClientLines {
    tx: std::sync::mpsc::Sender<ClientLine>,
    rx: std::sync::mpsc::Receiver<ClientLine>,
}

/// A SUPER+drag in flight (tiling.lua: mouse:272 moves, mouse:273 resizes).
pub struct DragState {
    client: ClientId,
    /// Right button: resize instead of move.
    resize: bool,
    resize_x: bool,
    resize_y: bool,
    /// The window was floating when the drag began.
    floating: bool,
    start: Vec2d,
    last: Vec2d,
    start_rect: LRect,
    /// Which corner the grab belongs to, by the QUADRANT of the grab point
    /// around the window center (DragController.cpp:213-226). The opposite
    /// corner stays put while the drag moves these two edges.
    grab_left: bool,
    grab_top: bool,
    /// `binds:drag_threshold` — a press is not a drag until it moves.
    armed: bool,
    /// SHIFT as the pointer last reported it. A drop with SHIFT held makes
    /// the dragged window a TAB of the tile under the pointer instead of
    /// swapping the two, so the flag has to be read from the drag's own
    /// events, not from the keyboard state at some later moment.
    shift: bool,
}

/// Hyprland's `binds:drag_threshold` (DragController.cpp:346).
const DRAG_THRESHOLD: f64 = 3.0;

/// A divider drag in flight: pressing IN THE GAP between two tiles grabs
/// the split that draws that gap and moves it with the pointer.
///
/// This is Hyprland's `resize_on_border` gesture scoped to the gap. On the
/// border it competes with every click near a window edge, which is why
/// omarchy ships `resize_on_border = false` (`default/hypr/looknfeel.lua`);
/// in the gap there is nothing to compete with, so it needs no modifier
/// and leaves the SUPER paths untouched — SUPER+drag still moves/swaps a
/// window, SUPER+right-drag still does the quadrant resize.
pub struct DividerDrag {
    /// The split, its box and its ratio AT THE GRAB — the move is measured
    /// from here every frame, never accumulated, so the divider stays
    /// exactly under the pointer.
    hit: DividerHit,
    start: Vec2d,
}

impl App {
    fn desk(&self, cx: &mut Cx) -> WidgetRef {
        self.ui.widget(cx, ids!(desk))
    }

    fn theme_name_from_env() -> String {
        let mut args = std::env::args();
        while let Some(arg) = args.next() {
            if arg == "--theme" {
                if let Some(name) = args.next() {
                    return name;
                }
            }
        }
        std::env::var("MAKEPAD_WM_THEME").unwrap_or_else(|_| {
            // Last chosen theme, omarchy-style state file.
            std::fs::read_to_string(theme::themes_dir().join("../current-theme"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| theme::DEFAULT_THEME.to_string())
        })
    }

    pub fn state_mut(&mut self) -> &mut WmState {
        self.state.as_mut().expect("state after startup")
    }

    fn desk_area(&self, cx: &mut Cx) -> LRect {
        let desk = self.desk(cx);
        let rect = desk
            .borrow_mut::<WmDesk>()
            .map(|d| d.desk_rect)
            .unwrap_or_default();
        let gap = self
            .state
            .as_ref()
            .map(|s| s.gaps_out)
            .unwrap_or(desk::GAPS_OUT);
        // Before the first draw the desk has no rect yet; fall back to the
        // window's startup proportions so the first splits pick the right
        // axis (A | B side-by-side on a wide screen).
        let (w, h) = if rect.size.x > 1.0 {
            (rect.size.x, rect.size.y)
        } else {
            (1400.0, 860.0)
        };
        LRect::new(
            rect.pos.x + gap,
            rect.pos.y + gap,
            (w - gap * 2.0).max(1.0),
            (h - gap * 2.0 - self.state.as_ref().map(|s| s.style.reserved_height()).unwrap_or(0.0)).max(1.0),
        )
    }

    // --------------------------------------------------------------
    // Client lifecycle
    // --------------------------------------------------------------

    fn launch_app(&mut self, cx: &mut Cx, app_id: &str) {
        self.launch_app_with_args(cx, app_id, &[]);
    }

    /// `launch_app` with arguments appended to the app's own — a `Launch`
    /// request's `args`, URLs for the Browser. Arguments mean a NEW window:
    /// neither a running instance nor a warm one, which started without
    /// them, can take them, so both shortcuts stand aside.
    fn launch_app_with_args(&mut self, cx: &mut Cx, app_id: &str, extra_args: &[String]) {
        let Some(app) = crate::clients::find_app(app_id) else {
            log!("octosense: no app '{}' in the registry", app_id);
            self.notify(cx, "App unavailable", &format!("Register '{app_id}' in your app catalog to launch it."));
            return;
        };
        let app = &app;
        // An installed app opens only while the App Hub catalog still admits
        // it. A system app (`os.*`) ships with the build and answers to no
        // catalog.
        // No script app runs under a native app's name (ADR 0004 §3).
        if let Some(Err(error)) = apps::card_manifest_id(app).map(apps::check_script_app_id) {
            self.notify(cx, "Could not open app", &error);
            return;
        }
        #[cfg(any(feature = "app-hub", native_mobile))]
        if let Some(manifest_id) = apps::card_manifest_id(app).filter(|id| !id.starts_with("os.")) {
            if let Err(error) = octosense_app_hub_app::catalog::try_may_open_from_environment(
                octosense_app_hub_app::data_root(cx), manifest_id) {
                self.notify(cx, "Could not open app", &error);
                return;
            }
        }
        // Its manifest's storage block (ADR 0004 §11) lays out its folders
        // before the Card runner opens it, system apps included.
        #[cfg(any(feature = "app-hub", native_mobile))]
        if let (Some(manifest_id), Some(storage)) = (apps::card_manifest_id(app), app_storage::host()) {
            if let Err(e) = app_storage::lifecycle::prepare_script_app(storage, storage.layout().apps_root(), manifest_id) {
                log!("app storage: {e}");
            }
        }
        let hub_port = self.state_mut().hub_port;

        // launch-or-focus for non-terminal apps: `omarchy-launch-or-focus`
        // matches `\b<pattern>\b` case-insensitively against the window's
        // CLASS OR TITLE and focuses the first hit. A Card app focuses only
        // an instance of itself (`apps::matches_running_app`).
        if app.policy == LaunchPolicy::OrFocus && extra_args.is_empty() {
            let mut existing: Vec<ClientId> = self
                .state_mut()
                .clients
                .iter()
                .filter(|(_, slot)| {
                    // A warm instance is NOT a window: matching one here
                    // would "focus" something invisible and the app would
                    // never open at all. Nor is the AI pane's child.
                    !slot.warm
                        && !slot.pane
                        && !slot.is_preview
                        && slot.closing.is_none()
                        && apps::matches_running_app(app, &slot.app, &slot.title)
                })
                .map(|(id, _)| *id)
                .collect();
            // `head -n1` over hyprctl's client list: take the oldest.
            existing.sort_unstable();
            let existing = existing.first().copied();
            if let Some(client) = existing {
                self.activate_client(cx, client);
                return;
            }
        }

        // In-process hosting (aicontrol §3): a linked module the person
        // switched on (or a dev `--module` run) becomes an instance in an
        // isolate of its own — never a process, never the pool.
        if self.apps.hosting(app_id) == Hosting::Module {
            if let Some(module) = self.apps.module(app_id) {
                if !extra_args.is_empty() {
                    // The gap: `launch_module` takes no arguments, so a
                    // sender that needs them must open the module through
                    // its OpenSchema instead.
                    log!("wm: {} runs in-process; launch arguments {:?} are not forwarded", app_id, extra_args);
                }
                self.launch_module_as(cx, module, app);
                return;
            }
        }

        // From here it is a PROCESS: nothing a build without a process
        // host can do (the web hosts its linked modules and nothing else).
        if !host::processes_available() {
            log!("wm: {} is not linked into this build; it cannot start here", app_id);
            return;
        }

        // THE POOL: a standby instance of this app becomes the window now,
        // already drawn, and the pool tops itself back up behind it.
        if extra_args.is_empty() && self.adopt_warm(cx, app_id) {
            return;
        }

        let id = self.next_id;
        self.next_id += 1;
        let cwd = self.launch_cwd(app_id, true);
        let state = self.state_mut();
        let term_env = state.term_env.clone();
        let lines = self.line_sender();
        let pool = cx.task_pool();
        let state = self.state_mut();
        match spawn_client(
            &pool,
            &cx.thread_spawner(),
            app,
            id,
            hub_port,
            cwd.as_ref(),
            (app.id == "terminal").then_some(term_env.as_str()),
            extra_args,
            false,
            lines,
        ) {
            Ok(slot) => {
                log!("wm: launched {} as client {}", app.id, id);
                state.clients.insert(id, slot);
                let area = self.desk_area(cx);
                let state = self.state_mut();
                let gap = state.gap;
                state.layout.insert(id, area, gap);
                let hub_port = state.hub_port;
                self.desk(cx)
                    .borrow_mut::<WmDesk>()
                    .map(|mut d| d.with_run_view(cx, id, |cx, v| v.set_run_target(cx, id, 0, hub_port)));
                self.activate_client(cx, id);
                self.redraw_all(cx);
            }
            Err(err) => {
                log!("wm: launch {} failed: {}", app.id, err);
                self.notify(cx, "Could not open app", &format!("{}: {err}", app.label));
            }
        }
    }

    // --------------------------------------------------------------
    // The warm pool
    //
    // A new terminal or browser should APPEAR, not launch. One dormant
    // instance of each pooled app (two for terminals) is already running,
    // connected and drawn into a framebuffer of its own; opening a window
    // hands it a tile and wakes it up, and the pool immediately starts its
    // replacement for the next time.
    //
    // The mechanisms are Quick Look's, not new ones: a client that is not
    // in the LAYOUT has no tile, is not drawn, and is invisible to
    // everything the desk enumerates (alt-tab, workspace counts, the bar's
    // active window) — exactly how a hidden warm viewer waits between
    // panels — plus `takes_focus: false`, which is what keeps a preview
    // from stealing the keyboard, and the same `spawn_client` path with
    // the same cargo, env and per-client log.
    // --------------------------------------------------------------

    /// Where a new window of `app_id` opens.
    ///
    /// `inherit` is omarchy's terminal rule (`omarchy-cmd-terminal-cwd`):
    /// a new terminal opens in the FOCUSED terminal's directory. A warm
    /// instance passes `false` — its shell started long before the launch
    /// and cannot be moved after the fact — which is exactly why the pool
    /// stands aside whenever there is a cwd to inherit.
    fn launch_cwd(&mut self, app_id: &str, inherit: bool) -> Option<std::path::PathBuf> {
        if app_id != "terminal" {
            return None;
        }
        let focused = if inherit {
            self.state_mut().focused_terminal_cwd()
        } else {
            None
        };
        focused.or_else(|| {
            // In demo mode a terminal with nothing to inherit opens inside
            // the generated demo home so `ls` shows plausible content,
            // never the user's real files.
            if octosense::policy::requested("--demo-home") {
                crate::demo_home::ensure_demo_home()
            } else {
                // Otherwise the person's home, like any terminal: never the
                // directory the launch happens to run in (a process
                // Terminal is started from its catalog row's checkout).
                user_home()
            }
        })
    }

    /// The pool's half of a launch. True when a standby instance became
    /// the window (the caller is done); false to launch cold exactly as
    /// before — the pool is an optimization and never the only path.
    fn adopt_warm(&mut self, cx: &mut Cx, app_id: &str) -> bool {
        if !WarmPool::is_warm_app(app_id) {
            return false;
        }
        // THE CWD CARVE-OUT: correctness beats instant. A new terminal
        // opened from a terminal must land in that terminal's directory,
        // and no warm shell can claim to have started there.
        let cwd_override = app_id == "terminal" && self.state_mut().focused_terminal_cwd().is_some();
        let status: Vec<WarmStatus> = self
            .state_mut()
            .clients
            .iter()
            .map(|(id, slot)| WarmStatus {
                client: *id,
                alive: true,
                // Connected AND past CreateWindow: it has a window and a
                // frame, so a tile can show it this instant.
                connected: slot.sender.is_some() && slot.ready && slot.closing.is_none(),
            })
            .collect();
        let Some(client) = self.warm_pool.adopt(app_id, cwd_override, &status) else {
            if cwd_override {
                log!("wm: warm {} skipped — new terminal inherits a cwd", app_id);
            }
            return false;
        };

        // It is a real window from here: out of the pool, into the layout
        // at the ordinary insertion point, focused like any launch.
        let area = self.desk_area(cx);
        let window_id = self
            .state_mut()
            .clients
            .get(&client)
            .map(|s| s.window_id)
            .unwrap_or(0);
        {
            let state = self.state_mut();
            if let Some(slot) = state.clients.get_mut(&client) {
                slot.warm = false;
                slot.takes_focus = true;
                slot.open_at = Some(host::now());
                slot.opened_warm = true;
            }
            let gap = state.gap;
            state.layout.insert(client, area, gap);
        }
        let hub_port = self.state_mut().hub_port;
        // The tile configures the child to its REAL rect on its next tick
        // (one WindowGeomChange plus the tile's own swapchain) — the same
        // bootstrap every launched client gets, except this one is already
        // running, so the arrival crossfade is all the user sees.
        self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
            d.with_run_view(cx, client, |cx, v| {
                v.set_run_target(cx, client, window_id, hub_port);
                v.app_ready(cx, client, window_id);
            })
        });
        self.activate_client(cx, client);
        // Wake it: samplers, refresh timers, everything a dormant instance
        // was told to hold back (`makepad_wm_api::warm_start`).
        self.send_wm_event(client, WmEvent::Adopted);
        // The tile owns the framebuffer now; the warm one is held briefly
        // in case a frame is still in flight against it.
        if let Some(frame) = self.warm_frames.get_mut(&client) {
            frame.retired = Some(host::now());
        }
        log!("wm: adopted warm {} as client {}", app_id, client);
        self.redraw_all(cx);
        // …and the next one starts warming immediately.
        self.spawn_warm(cx, app_id);
        true
    }

    /// Put one dormant instance of `app_id` in the pool. Quiet by design:
    /// the pool is invisible infrastructure, and anything it cannot do
    /// simply means the next launch of that app is a cold one.
    fn spawn_warm(&mut self, cx: &mut Cx, app_id: &str) {
        if !self.warm_pool.wants(app_id, host::now()) {
            return;
        }
        // Let retired browsers release their profile lock before a replacement
        // starts. This also coalesces quick light/dark toggles into one launch
        // with the latest child environment.
        if app_id == "browser" && self.state_mut().clients.values().any(|slot|
            slot.app == "browser" && slot.warm && slot.closing.is_some()) {
            return;
        }
        let Some(app) = crate::clients::find_app(app_id) else {
            return;
        };
        let hub_port = self.state_mut().hub_port;
        if hub_port == 0 {
            // No hub: a child could never connect, so warming is pointless.
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        let cwd = self.launch_cwd(app_id, false);
        let term_env = self.state_mut().term_env.clone();
        let lines = self.line_sender();
        let pool = cx.task_pool();
        match spawn_client(
            &pool,
            &cx.thread_spawner(),
            &app,
            id,
            hub_port,
            cwd.as_ref(),
            (app.id == "terminal").then_some(term_env.as_str()),
            &[],
            true,
            lines,
        ) {
            Ok(slot) => {
                self.state_mut().clients.insert(id, slot);
                self.warm_pool.note_spawned(app_id, id);
                log!("wm: warming {} as client {}", app_id, id);
            }
            Err(err) => {
                // A spawn that cannot even start (an installed layout without
                // this binary) counts as a crash: the budget gives the app up
                // after a few, instead of retrying it on every tick forever.
                self.warm_pool.note_crash(app_id, host::now());
                log!("wm: warming {} failed: {}", app_id, err);
            }
        }
    }

    /// Top the pool up, ONE spawn per tick: a cold desktop otherwise forks
    /// four cargo builds into the same target-dir lock at once, and they
    /// would only queue behind each other anyway.
    fn top_up_warm_pool(&mut self, cx: &mut Cx) {
        let Some(app) = self.warm_pool.next_missing(host::now()) else {
            return;
        };
        self.spawn_warm(cx, &app);
    }

    /// Hand a dormant instance a geometry and a framebuffer of its own so
    /// it does its FIRST FULL DRAW now, with nobody waiting: shaders
    /// compiled, glyph atlas rasterized, UI laid out. Sized at the desk's
    /// full-tile rect — the common case is a first window, and any other
    /// size costs exactly one reconfigure at adoption.
    fn warm_bootstrap(&mut self, cx: &mut Cx, client: ClientId, window_id: usize) {
        // Linux shares a swapchain over an aux-chan socket owned by the
        // TILE (`MpRunView::setup_aux_chan`), and a warm instance has no
        // tile; there an instance stays process-warm and gets its
        // framebuffer at adoption. mac and Windows share by IOSurface /
        // HANDLE, which needs no channel.
        #[cfg(all(target_os = "linux", not(target_env = "ohos")))]
        let swapchain: Option<HostSwapchain> = {
            let _ = (window_id, &cx);
            None
        };
        #[cfg(not(all(target_os = "linux", not(target_env = "ohos"))))]
        let swapchain: Option<HostSwapchain> = {
            let rect = self.desk_area(cx);
            let dpi = if self.dpi_factor > 0.0 {
                self.dpi_factor
            } else {
                1.0
            };
            let (w, h) = (rect.w.max(1.0), rect.h.max(1.0));
            // The tile's own allocation rule, so a warm instance warms up
            // at the size a full-desk tile would have asked for.
            let alloc = |px: f64| ((px * dpi).ceil() as u32).max(64).next_power_of_two();
            let swapchain = HostSwapchain::new(window_id, alloc(w), alloc(h), cx);
            let shared = shared_swapchain_from_host_swapchain(&swapchain, cx);
            let msgs = vec![
                StudioToApp::WindowGeomChange {
                    window_id,
                    dpi_factor: dpi,
                    left: 0.0,
                    top: 0.0,
                    width: w,
                    height: h,
                },
                StudioToApp::Swapchain(shared),
            ];
            if let Some(sender) = self
                .state_mut()
                .clients
                .get(&client)
                .and_then(|s| s.sender.clone())
            {
                send_to_app(&sender, msgs);
            }
            Some(swapchain)
        };
        self.warm_frames.insert(
            client,
            WarmFrame {
                swapchain,
                presented: false,
                retired: None,
            },
        );
    }

    /// Drive the dormant instances, and only as far as they need driving.
    ///
    /// A `--stdin-loop` child does ALL of its work inside
    /// `StudioToApp::Tick` — it flushes its platform ops there (which is
    /// how a window is ANNOUNCED at all), runs its timers, draws and
    /// repaints — and between ticks it blocks on its socket at no cost
    /// whatsoever. A tile pumps its child every 8ms; a warm instance has
    /// no tile, so the WM pumps it here instead: at the pump rate until it
    /// has drawn its first frame — without those first ticks it would
    /// never even send `CreateWindow`, and the pool would deadlock waiting
    /// for a window that needs a tick to exist — and a once-a-second
    /// heartbeat after that. Together with `MAKEPAD_WM_WARM_START` (the app's
    /// own half of the deal: no samplers, no refresh while dormant) that
    /// is what keeps a cached task manager from burning a core in the
    /// background.
    fn pump_warm(&mut self, _cx: &mut Cx) {
        let now = host::now();
        // A retired framebuffer outlives adoption only long enough for any
        // frame still in flight against it.
        self.warm_frames.retain(|_, frame| {
            frame
                .retired
                .map(|t| now - t < WARM_FRAME_RETIRE)
                .unwrap_or(true)
        });
        let warm = self.warm_pool.clients();
        self.warm_last_tick.retain(|client, _| warm.contains(client));
        let due: Vec<ClientId> = warm
            .into_iter()
            .filter(|client| {
                // Still coming up (no window yet), or it has a framebuffer
                // and has not drawn into it: full rate. Anything else —
                // drawn, or a platform where a warm instance cannot be
                // handed a framebuffer at all — only needs the heartbeat.
                let warming = match self.warm_frames.get(client) {
                    None => true,
                    Some(frame) => frame.swapchain.is_some() && !frame.presented,
                };
                match self.warm_last_tick.get(client) {
                    Some(t) if !warming => now - *t >= WARM_HEARTBEAT,
                    _ => true,
                }
            })
            .collect();
        for client in due {
            let Some(sender) = self
                .state_mut()
                .clients
                .get(&client)
                .and_then(|s| s.sender.clone())
            else {
                // Not connected yet (still building, still starting): the
                // pump has nothing to send it down.
                continue;
            };
            self.warm_last_tick.insert(client, now);
            send_to_app(&sender, vec![StudioToApp::Tick]);
        }
    }

    /// The pool's honest measurement: how long from the launch gesture to
    /// this window's first frame ON THE DESK, and by which path.
    fn note_first_frame(&mut self, client: ClientId) {
        let Some(slot) = self.state_mut().clients.get_mut(&client) else {
            return;
        };
        let Some(at) = slot.open_at.take() else {
            return;
        };
        let (app, warm) = (slot.app.clone(), slot.opened_warm);
        log!(
            "wm: {} client {} first frame in {} ms ({})",
            app,
            client,
            ((host::now() - at) * 1000.0) as u64,
            if warm { "warm" } else { "cold" }
        );
    }

    /// The channel children write their output into (created lazily so
    /// the very first launch in handle_startup already has one).
    fn line_sender(&mut self) -> std::sync::mpsc::Sender<ClientLine> {
        if self.client_lines.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            self.client_lines = Some(ClientLines { tx, rx });
        }
        self.client_lines.as_ref().unwrap().tx.clone()
    }

    /// Put every child's newest output line on its tile — cargo's
    /// "Compiling …" while the app is still being built.
    fn drain_client_lines(&mut self, cx: &mut Cx) {
        let mut latest: Vec<(ClientId, String, bool)> = Vec::new();
        let lines: Vec<_> = self.client_lines.as_ref()
            .map(|lines| lines.rx.try_iter().collect()).unwrap_or_default();
        for line in lines {
            // Keep raw output for failure diagnostics even when it is not a
            // Cargo progress message suitable for the startup tile.
            if let Some(slot) = self.state_mut().clients.get_mut(&line.client) {
                slot.diagnostic = line.text.clone();
            }
            let Some((text, linked)) = clients::cargo_progress(&line.text) else { continue };
            match latest.iter_mut().find(|(c, _, _)| *c == line.client) {
                Some(slot) => { slot.1 = text; slot.2 |= linked; }
                None => latest.push((line.client, text, linked)),
            }
        }
        for (client, text, linked) in latest {
            let mut warm = false;
            let mut pane = false;
            if let Some(slot) = self.state_mut().clients.get_mut(&client) {
                warm = slot.warm;
                pane = slot.pane;
                slot.status = text.clone();
                // cargo's last word before the app takes over. macOS then
                // scans a freshly linked binary on its FIRST exec, which
                // can hold main() for tens of seconds — see the tick.
                // Only cargo's handover means the binary is fresh on disk
                // and about to be exec'd for the first time.
                slot.linked |= linked;
                if linked {
                    slot.linked_at = Some(host::now());
                }
            }
            // A warm instance has no tile to put a status line on, and
            // asking the desk for one would BUILD it — a widget nothing
            // draws, with an 8ms child-tick timer behind it. The pool's
            // invariant is literal: no tile until adoption.
            if warm {
                continue;
            }
            // The pane's child has its own run view, never a desk tile.
            if pane {
                self.with_pane_run_view(cx, |cx, v| v.set_status_line(cx, &text));
                continue;
            }
            self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
                d.with_run_view(cx, client, |cx, v| v.set_status_line(cx, &text))
            });
        }
    }

    /// Everything a hosted app can ask of the compositor.
    pub fn on_wm_request(&mut self, cx: &mut Cx, client: ClientId, req: WmRequest) {
        match &req {
            // Quick Look: the warm-viewer cache (`handle_preview_request`).
            // A plain `Open` still tiles a real window every time.
            WmRequest::Preview { app, path } => {
                self.handle_preview_request(cx, client, app.clone(), path.clone());
            }
            // A named app the catalog cannot launch here is answered, not
            // dropped: the requester falls back to what it can do itself.
            WmRequest::Open { app: Some(app), path } if !Self::launchable(app) => {
                self.reply_unavailable(cx, client, app, path);
            }
            WmRequest::Open { .. } => {
                if let Some(open) = preview::OpenRequest::from_request(&req) {
                    self.open_request(cx, Some(client), open);
                }
            }
            // The REQUESTER hiding its own panel. A stray close from
            // anyone else (or after the panel already moved on to a
            // different requester) is ignored.
            WmRequest::PreviewClose => {
                if self
                    .preview_cache
                    .active
                    .as_ref()
                    .map(|a| a.requester == client)
                    .unwrap_or(false)
                {
                    self.hide_active_preview(cx);
                }
            }
            // A requester that predates the envelope (Files asking for a
            // Terminal) hears nothing from the reply, so the person is told
            // as `launch_app` tells them; an `Open` requester handles the
            // reply itself.
            WmRequest::Launch { app, .. } if !Self::launchable(app) => {
                self.reply_unavailable(cx, client, app, "");
                self.notify(cx, "App unavailable", &format!("Register '{app}' in your app catalog to launch it."));
            }
            WmRequest::Launch { app, args } => {
                let (app, args) = (app.clone(), args.clone());
                self.launch_app_with_args(cx, &app, &args);
            }
            WmRequest::Title { title } => {
                if let Some(slot) = self.state_mut().clients.get_mut(&client) {
                    slot.title = title.clone();
                }
                self.update_bar(cx);
            }
            WmRequest::Cwd { path } => {
                if let Some(slot) = self.state_mut().clients.get_mut(&client) {
                    slot.pwd = Some(std::path::PathBuf::from(path));
                }
            }
            WmRequest::Notify { title, body } => {
                log!("wm: notify from client {}: {} — {}", client, title, body);
                let (app, now) = (self.state_mut().clients.get(&client).map(|s| s.app.clone()).unwrap_or_default(), cx.seconds_since_app_start());
                self.state_mut().phone.shade.post(&app, title, body, now, vec!["Open".into()]);
            }
            // The app itself is done (or its person just confirmed): no
            // question goes back to it.
            WmRequest::Close => self.close_client(cx, client, false),
            WmRequest::SetFloating { floating } => {
                let area = self.desk_area(cx);
                let gap = self.state_mut().gap;
                let is_float = self.state_mut().layout.is_float(client);
                if is_float != *floating {
                    self.state_mut().layout.toggle_float(client, area, gap);
                    self.redraw_all(cx);
                }
            }
            WmRequest::SetFullscreen { fullscreen } => {
                self.focus_client(cx, client);
                let on = self.state_mut().layout.fullscreen_mode() != FullscreenMode::None;
                if on != *fullscreen {
                    self.do_action(cx, WmAction::Fullscreen(FullscreenMode::Fullscreen));
                }
            }
            // Words a newer makepad adds (the close answers are read before
            // this parse, by `process_close::parse_close_word`).
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    /// A client asked us to open a file in its associated app as a normal
    /// tiled window (`WmRequest::Open`). `WmRequest::Preview` (Quick Look)
    /// goes through `handle_preview_request`'s warm-viewer cache instead.
    /// `requester` is the client to answer when nothing opens (a spawn
    /// failure, a module-only target on a host without processes); the
    /// assistant's `open` tool has none, its result says what happened.
    fn open_request(&mut self, cx: &mut Cx, requester: Option<ClientId>, req: preview::OpenRequest) {
        let hub_port = self.state_mut().hub_port;
        let id = self.next_id;
        self.next_id += 1;
        let lines = self.line_sender();
        let pool = cx.task_pool();
        let slot = match preview::spawn_for_request(&pool, &cx.thread_spawner(), &req, id, hub_port, lines) {
            Ok(slot) => slot,
            Err(err) => {
                log!("wm: open request failed: {}", err);
                if let Some(client) = requester {
                    self.reply_unavailable(cx, client, &req.app, &req.path.to_string_lossy());
                }
                return;
            }
        };
        log!("wm: opening {} as client {}", req.path.display(), id);
        self.state_mut().clients.insert(id, slot);
        self.sync_geometry(cx);
        let area = self.desk_area(cx);
        let state = self.state_mut();
        let gap = state.gap;
        state.layout.insert(id, area, gap);
        let hub_port = state.hub_port;
        self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
            d.with_run_view(cx, id, |cx, v| v.set_run_target(cx, id, 0, hub_port))
        });
        self.focus_client(cx, id);
        self.redraw_all(cx);
    }

    /// Send a hosted client a `WmEvent` over its own studio socket — the
    /// WM->app half of the `makepad_wm_api` protocol. False when the client is
    /// gone or has not connected yet, so a caller whose message MUST land
    /// (a Quick-Look retarget) can park it for `HubEvent::Connected`.
    fn send_wm_event(&mut self, client: ClientId, ev: WmEvent) -> bool {
        self.send_custom_to_client(client, ev.to_json())
    }

    /// One JSON message to a process client as `StudioToApp::Custom`.
    /// False when the client is gone or has no socket yet.
    fn send_custom_to_client(&mut self, client: ClientId, json: String) -> bool {
        if let Some(slot) = self.state_mut().clients.get(&client) {
            if let Some(sender) = &slot.sender {
                crate::hub::send_to_app(sender, vec![StudioToApp::Custom(json)]);
                return true;
            }
        }
        false
    }

    /// Whether `app` can be launched from this catalog on this host: a
    /// linked module when hosted as one, else an available process.
    fn launchable(app: &str) -> bool {
        clients::find_app(app).is_some_and(|def| crate::apps::is_launchable(&def))
    }

    /// Tell the requester that `app` cannot be launched from this catalog:
    /// the `wm_unavailable` envelope, into a module's isolate or over a
    /// process's socket. What to do instead is the requester's call.
    fn reply_unavailable(&mut self, cx: &mut Cx, client: ClientId, app: &str, path: &str) {
        let json = wm_reply::WmUnavailable { app: app.to_string(), path: path.to_string() }.to_json();
        let told = if self.module_host.is_module(client) {
            self.module_host.send_custom(cx, client, json)
        } else {
            self.send_custom_to_client(client, json)
        };
        log!("wm: {app} is not launchable here; told client {client}: {told}");
    }

    /// Retarget a warm viewer at `path`. A viewer spawned moments ago has
    /// no socket yet — park the newest file and let `drain_hub` flush it
    /// the instant the child connects, or a fast arrow-dial silently keeps
    /// the file the viewer was launched with.
    fn send_preview_file(&mut self, client: ClientId, path: &str) {
        log!("wm: preview retarget client {} -> {}", client, path);
        let ev = WmEvent::PreviewFile {
            path: path.to_string(),
        };
        if !self.send_wm_event(client, ev) {
            self.preview_cache.queue_pending(client, path.to_string());
        }
    }

    /// `WmRequest::Preview`: Quick Look through the warm-viewer cache.
    /// Reuses a live viewer of the right TYPE if one exists — sending it
    /// `PreviewFile` to retarget in place, no respawn, no popin, no lost
    /// selection — else spawns one exactly as before and remembers it.
    /// Switching type hides (not kills) whatever was showing. The
    /// requester keeps key focus the entire time (the FOCUS RULE) and is
    /// told `PreviewShown` once the panel is actually up.
    fn handle_preview_request(
        &mut self,
        cx: &mut Cx,
        requester: ClientId,
        app: Option<String>,
        path: String,
    ) {
        let Some(open) = preview::OpenRequest::from_request(&WmRequest::Preview { app, path })
        else {
            return;
        };
        let viewer_app = open.app.clone();
        let path_str = open.path.to_string_lossy().to_string();

        // A different TYPE is currently the visible panel: hide it (it
        // stays warm) before showing this one. The requester is NOT told
        // `PreviewHidden` here — from its side the panel never goes away,
        // it just changes what it shows, and a Hidden/Shown pair inside one
        // keystroke would make its event-driven state stutter.
        //
        // That silence has to be paid back if the new viewer never comes
        // up: the requester would go on believing a panel is there, its
        // Space would close a panel that is not, and Quick Look would wedge
        // until something else resynced it. `suppressed_hide` says the debt
        // is outstanding; every failure path below settles it.
        let mut suppressed_hide = false;
        if self.preview_cache.switching_type(&viewer_app) {
            self.hide_active_preview_ex(cx, false);
            suppressed_hide = true;
        }

        // Reuse this type's warm viewer if the process behind it is still
        // usable; a dead or CLOSING one clears its own slot in there, so a
        // viewer already on its way out never gets the next file.
        let alive: Vec<ClientId> = self
            .state_mut()
            .clients
            .iter()
            .filter(|(_, s)| s.closing.is_none())
            .map(|(id, _)| *id)
            .collect();
        let warm = self
            .preview_cache
            .warm_client(&viewer_app, |id| alive.contains(&id));
        if let Some(id) = warm {
            self.send_preview_file(id, &path_str);
            if !self.preview_cache.is_showing(id) {
                // Was hidden (or this is its first request this session
                // after a hide) — bring the float back, same reused rect.
                self.show_preview_float(cx, id);
            }
            self.preview_cache.active = Some(preview::ActivePreview {
                requester,
                viewer_app,
                client: id,
                path: open.path.clone(),
            });
            self.send_wm_event(requester, WmEvent::PreviewShown { path: path_str });
            self.redraw_all(cx);
            return;
        }

        // Spawn fresh, exactly as an ordinary preview always has, except it
        // never takes focus and is remembered as this type's warm slot.
        let hub_port = self.state_mut().hub_port;
        let id = self.next_id;
        self.next_id += 1;
        let lines = self.line_sender();
        let pool = cx.task_pool();
        let slot = match preview::spawn_for_request(&pool, &cx.thread_spawner(), &open, id, hub_port, lines) {
            Ok(mut slot) => {
                slot.is_preview = true;
                slot.takes_focus = false;
                slot
            }
            Err(err) => {
                log!("wm: preview request failed: {}", err);
                if suppressed_hide {
                    self.send_wm_event(requester, WmEvent::PreviewHidden);
                }
                return;
            }
        };
        log!(
            "wm: previewing {} as client {} ({})",
            open.path.display(),
            id,
            viewer_app
        );
        self.state_mut().clients.insert(id, slot);
        self.preview_cache.remember(&viewer_app, id);
        self.sync_geometry(cx);
        let hub_port = self.state_mut().hub_port;
        self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
            d.with_run_view(cx, id, |cx, v| v.set_run_target(cx, id, 0, hub_port))
        });
        self.show_preview_float(cx, id);
        self.preview_cache.active = Some(preview::ActivePreview {
            requester,
            viewer_app,
            client: id,
            path: open.path,
        });
        self.send_wm_event(requester, WmEvent::PreviewShown { path: path_str });
        self.redraw_all(cx);
    }

    /// Float a (warm or fresh) preview client into the single reused
    /// popup rect — `popup_rect` is a pure function of the desk, so every
    /// show of every viewer type lands on exactly the same rect and a
    /// retarget never moves the panel. `add_preview_float` keeps the
    /// layout's focus where it was (the FOCUS RULE), and the tile itself
    /// is marked so its own clicks cannot take the keyboard either.
    fn show_preview_float(&mut self, cx: &mut Cx, client: ClientId) {
        let area = self.desk_area(cx);
        let state = self.state_mut();
        let gap = state.gap;
        let ws = state.layout.active;
        let rect = state.layout.popup_rect(area, gap, 900.0, 700.0);
        state.layout.add_preview_float(client, rect, ws);
        self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
            d.with_run_view(cx, client, |_cx, v| v.set_takes_key_focus(false))
        });
        self.redraw_all(cx);
    }

    /// Hide the active Quick Look panel: the float goes away, the warm
    /// viewer is told to `PreviewUnload` (drop decoders/textures, idle)
    /// but is NEVER killed here, and the requester is told `PreviewHidden`.
    fn hide_active_preview(&mut self, cx: &mut Cx) {
        self.hide_active_preview_ex(cx, true);
    }

    /// `notify_requester` is false only for the hide inside a TYPE SWITCH,
    /// where the panel is about to come straight back with another viewer:
    /// the requester is told `PreviewShown` for the new file instead, and
    /// never sees a phantom close.
    fn hide_active_preview_ex(&mut self, cx: &mut Cx, notify_requester: bool) {
        let Some(active) = self.preview_cache.active.take() else {
            return;
        };
        // Out of the layout, but NOT out of the desk: `WmDesk::remove_client`
        // would drop the tile widget (and with it the child's swapchain and
        // this viewer's warmth) once its close animation finished.
        self.state_mut().layout.remove(active.client);
        log!(
            "wm: preview hide client {} ({}), viewer stays warm",
            active.client,
            active.viewer_app
        );
        self.send_wm_event(active.client, WmEvent::PreviewUnload);
        // A retarget parked for a viewer that never connected dies with the
        // panel: showing it later would resurrect a file nobody asked for.
        self.preview_cache.take_pending(active.client);
        if notify_requester {
            self.send_wm_event(active.requester, WmEvent::PreviewHidden);
        }
        self.focus_after_layout(cx);
        self.redraw_all(cx);
    }

    /// Escape / Space dismisses the active Quick-Look panel, WM-level —
    /// handled here rather than forwarded to whatever has key focus,
    /// because under the FOCUS RULE that is always the REQUESTER (files),
    /// never the preview, so this can no longer key off
    /// `layout.focused_client()` the way it used to.
    fn close_focused_preview(&mut self, cx: &mut Cx) -> bool {
        if self.preview_cache.active.is_none() {
            return false;
        }
        // Only the app that is dialing loses its Space/Escape to the panel.
        // Focus somewhere else (a terminal the user tabbed to while the
        // panel stayed up) means Space is a space again.
        let focus = self.state_mut().layout.focused_client();
        match focus {
            Some(client) if !self.preview_cache.is_requester(client) => return false,
            _ => {}
        }
        self.hide_active_preview(cx);
        true
    }

    /// SUPER+W / SUPER+Q — `hl.dsp.window.close()`: a POLITE close the app
    /// honors itself. The tile leaves the layout at once (so the rest
    /// reflow and the close animation plays with its last frame), the
    /// process gets `CLOSE_GRACE` to exit, and only then is it killed.
    fn close_focused(&mut self, cx: &mut Cx) {
        let Some(focus) = self.state_mut().layout.focused_client() else {
            return;
        };
        self.request_close(cx, focus);
    }

    /// App Hub finished installing (or updating) `id`. An update replaces
    /// the bundle on disk, but a running Card keeps its old code and
    /// permissions: end its instances so the next open takes the new one.
    #[cfg(any(feature = "app-hub", native_mobile))]
    fn installed_app_changed(&mut self, cx: &mut Cx, id: &str) {
        // Its storage (ADR 0004 §11): an install records the manifest's
        // block and lays out the folders; a removal (the jail is gone)
        // deletes what the host keeps for it and keeps its agents suspended.
        if let Some(storage) = app_storage::host() {
            let root = storage.layout().apps_root();
            if !app_storage::lifecycle::app_uninstalled(storage, root, id) {
                if let Err(e) = app_storage::lifecycle::prepare_script_app(storage, root, id) {
                    log!("app storage: {e}");
                }
            }
        }
        let launch_id = apps::installed_launch_id(id);
        let clients: Vec<_> = self.state_mut().clients.iter()
            .filter_map(|(&client, slot)| (slot.app == launch_id).then_some(client))
            .collect();
        for client in clients {
            self.request_close(cx, client);
        }
        octosense_app_hub_app::icons::invalidate();
        crate::shell::launcher::invalidate_apps();
        self.redraw_all(cx);
    }

    fn request_close(&mut self, cx: &mut Cx, client: ClientId) {
        self.close_client(cx, client, true);
    }

    /// Close `client`. `ask`: a close the person (or the shell) asked for,
    /// which a process app that answers closes itself is asked about
    /// first. Not `ask`: the app's own `WmRequest::Close` — it decided, so
    /// it closes as any app always did (a hosted app cannot end its own
    /// process: `cx.quit()` does nothing under the Studio runtime).
    fn close_client(&mut self, cx: &mut Cx, client: ClientId, ask: bool) {
        // A close of one app drops a quit that was waiting on others (the
        // person turned to something else; module_host::CloseGate). An
        // app's own close is its answer, which a waiting quit counts on.
        if ask {
            self.module_host.abandon_quit();
            self.process_close.abandon_quit();
        }
        let stopped = self.state.as_ref().and_then(|s| s.clients.get(&client)).is_some_and(|s| s.stopped);
        // A live module instance is asked first (makepad#65): one holding
        // something a close would destroy (the terminal's running jobs)
        // refuses and asks the person in its own root. Its tile stays, in
        // front so the question is seen; a yes arrives as the root's
        // `ModuleCloseAction::Confirmed` (handle_actions), a no as nothing.
        // Insisting ends it whatever it would answer (process_close.rs,
        // `Insistence`): an instance that keeps refusing is never unclosable.
        let module = !stopped && self.module_host.is_module(client);
        let forced = module && ask && self.module_host.close_gate_mut().insist(client, host::now());
        if forced {
            log!("wm: client {client} was closed {} times in {}s; ending it", process_close::FORCE_CLOSES, process_close::FORCE_WINDOW);
        } else if module && self.module_host.ask_close(cx, client) == CloseDecision::Veto {
            let app = self.state.as_ref().and_then(|s| s.clients.get(&client)).map(|s| s.app.clone()).unwrap_or_default();
            log!("wm: {app} (client {client}) asks before closing; its tile stays until the person confirms");
            if self.module_host.close_gate_mut().next_close_forces(client) {
                self.hint_force_close(cx, client);
            }
            self.activate_client(cx, client);
            self.update_bar(cx);
            self.redraw_all(cx);
            return;
        }
        // A module instance has no process to ask politely and nothing to
        // reap later: it ends now, through the same removal as a death.
        // So does one of its extra windows, and a stopped process's tile.
        if stopped || self.module_host.is_module(client) || self.module_windows.contains_key(&client) {
            self.remove_client(cx, client);
            self.update_bar(cx);
            return;
        }
        // A process app that answers closes itself (it said
        // `AsksBeforeClose`): ask, and wait for its answer — an exit, or a
        // refusal while it asks the person. Its tile stays meanwhile.
        // Anything else closes as before, below (process_close.rs).
        let answerable = self
            .state
            .as_ref()
            .and_then(|s| s.clients.get(&client))
            .is_some_and(|s| s.sender.is_some() && s.closing.is_none() && !s.warm && !s.pane);
        if !ask {
            // Its answer came as the close itself: nothing left to wait on.
            self.process_close.gone(client);
        } else if answerable {
            match self.process_close.close(client, host::now()) {
                process_close::CloseStep::Legacy => {}
                process_close::CloseStep::Ask => {
                    let app = self.state_mut().clients.get(&client).map(|s| s.app.clone()).unwrap_or_default();
                    log!("wm: asking {app} (client {client}) to close; it answers before anything ends");
                    if self.process_close.next_close_forces(client) {
                        self.hint_force_close(cx, client);
                    }
                    self.send_wm_event(client, WmEvent::CloseRequested);
                    self.activate_client(cx, client);
                    self.update_bar(cx);
                    self.redraw_all(cx);
                    return;
                }
                process_close::CloseStep::Wait => return,
                process_close::CloseStep::Force => {
                    log!("wm: client {client} was closed again (before it answered, or insisting); ending it");
                    self.end_process_client(cx, client);
                    return;
                }
            }
        }
        let mut polite = false;
        if let Some(slot) = self.state_mut().clients.get_mut(&client) {
            if slot.closing.is_some() {
                return;
            }
            slot.closing = Some(host::now());
            if let Some(sender) = &slot.sender {
                // Ask over the API first (the app may want to save), then
                // the protocol's own Kill; the reaper is the fallback.
                send_to_app(
                    sender,
                    vec![
                        StudioToApp::Custom(WmEvent::CloseRequested.to_json()),
                        StudioToApp::Kill,
                    ],
                );
                polite = true;
            }
        }
        if !polite {
            // Never connected (still building): nothing to ask politely.
            let pool = cx.task_pool();
            if let Some(slot) = self.state_mut().clients.get_mut(&client) {
                if let Some(child) = slot.child.as_mut() {
                    clients::kill_child_group(child, clients::GROUP_KILL_GRACE, &pool);
                }
            }
        }
        self.leave_layout(cx, client);
    }

    /// Out of the layout now: the tiles reflow and the desk plays the
    /// popin-out with the frame the tile already has. The slot stays until
    /// the reaper sees the process gone.
    fn leave_layout(&mut self, cx: &mut Cx, client: ClientId) {
        self.state_mut().layout.remove(client);
        if let Some(mut desk) = self.desk(cx).borrow_mut::<WmDesk>() {
            desk.remove_client(client);
        }
        self.focus_after_layout(cx);
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// End a process app that was asked to close and did not answer (the
    /// person closed it again, or `process_close::ANSWER_TIMEOUT` passed):
    /// the protocol's Kill and the process group's SIGTERM, SIGKILL after
    /// `GROUP_KILL_GRACE`. A SIGTERM is a termination signal, which the
    /// terminal never refuses. Counted as asked for, not as a crash.
    fn end_process_client(&mut self, cx: &mut Cx, client: ClientId) {
        let pool = cx.task_pool();
        let Some(slot) = self.state_mut().clients.get_mut(&client) else {
            return;
        };
        slot.closing = Some(host::now());
        if let Some(sender) = &slot.sender {
            send_to_app(sender, vec![StudioToApp::Kill]);
        }
        if let Some(child) = slot.child.as_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                clients::kill_child_group(child, clients::GROUP_KILL_GRACE, &pool);
            }
        }
        self.leave_layout(cx, client);
    }

    /// An app's word about closes (makepad `WmRequest::AsksBeforeClose`
    /// / `CloseRefused`).
    fn on_close_word(&mut self, cx: &mut Cx, client: ClientId, word: process_close::CloseWord) {
        match word {
            process_close::CloseWord::AsksBeforeClose => {
                log!("wm: client {client} answers its closes itself");
                self.process_close.declare(client);
            }
            process_close::CloseWord::CloseRefused => {
                // Only an answer to a close the shell is waiting on counts.
                let first = self.process_close.asking() == 0;
                if !self.process_close.refused(client) {
                    return;
                }
                let app = self.state_mut().clients.get(&client).map(|s| s.app.clone()).unwrap_or_default();
                log!("wm: {app} (client {client}) asks before closing; its tile stays until the person answers");
                // A quit waiting on it waits from this refusal.
                if self.quit_waiting_since.is_some() {
                    self.quit_waiting_since = Some(host::now());
                }
                // In front so the question is seen; while a quit waits on
                // several, the first to ask keeps the front.
                if first || !self.process_close.quit_waiting() {
                    self.activate_client(cx, client);
                }
                self.update_bar(cx);
                self.redraw_all(cx);
            }
        }
    }

    /// The close before the one that ends an app that keeps refusing: say
    /// so, so the person knows another close ends it (and its unsaved work).
    fn hint_force_close(&mut self, cx: &mut Cx, client: ClientId) {
        let app = self.state.as_ref().and_then(|s| s.clients.get(&client)).map(|s| s.app.clone()).unwrap_or_default();
        let label = clients::find_app(&app).map(|a| a.label).unwrap_or(app);
        self.notify(cx, &format!("{label} is asking before it closes"), "Close it once more to end it now, without its answer.");
    }

    /// Whether `client` is answering a close or asking the person about
    /// one (a hosted module or a process app).
    fn close_pending(&self, client: ClientId) -> bool {
        self.module_host.close_pending(client) || self.process_close.is_pending(client)
    }

    /// A quit that waited on instances and apps asking the person may go
    /// ahead: every one of them confirmed (or went).
    fn take_quit_ready(&mut self) -> bool {
        process_close::take_quit_ready(self.module_host.close_gate_mut(), &mut self.process_close)
    }

    /// A module instance's root confirmed the close it refused earlier
    /// (`ModuleCloseAction::Confirmed` from the root's uid): it closes now,
    /// through the same removal as any close. A confirmation from a root
    /// whose close is not pending is ignored.
    fn module_close_confirmed(&mut self, cx: &mut Cx, uid: WidgetUid) {
        let Some(client) = self.module_host.close_confirmed(uid) else {
            return;
        };
        log!("wm: client {client} confirmed its close");
        self.remove_client(cx, client);
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// The shell is asked to quit (the menu's Quit, Cmd+Q, the window's
    /// close button): every hosted instance is asked first. True when
    /// nobody refused and the quit may go ahead now; otherwise the first
    /// refusing instance comes to the front with its question, and the
    /// quit happens when the last of them confirms (`take_quit_ready`). A
    /// forced end (a termination signal, a kill) never asks.
    fn ask_before_quit(&mut self, cx: &mut Cx) -> bool {
        let refused = self.module_host.ask_quit(cx);
        // An instance that refused this quit as often as insisting takes
        // (process_close::Insistence) is ended; the rest are waited on.
        let now = host::now();
        let (forced, refused): (Vec<ClientId>, Vec<ClientId>) =
            refused.into_iter().partition(|client| self.module_host.close_gate_mut().insist(*client, now));
        for client in forced {
            log!("wm: client {client} refused the quit {} times; ending it", process_close::FORCE_CLOSES);
            self.remove_client(cx, client);
        }
        // Process apps that answer closes themselves are asked the same way
        // (process_close.rs); the rest end with the shell, as always.
        let windows: Vec<ClientId> = self
            .state
            .as_ref()
            .map(|s| {
                s.clients
                    .iter()
                    .filter(|(_, s)| s.sender.is_some() && s.closing.is_none() && !s.warm && !s.pane && !s.stopped)
                    .map(|(client, _)| *client)
                    .collect()
            })
            .unwrap_or_default();
        let ask = self.process_close.quit(windows, host::now());
        for client in &ask.asked {
            self.send_wm_event(*client, WmEvent::CloseRequested);
        }
        for client in &ask.forced {
            log!("wm: client {client} was asked to close again before it answered; ending it");
            self.end_process_client(cx, *client);
        }
        if refused.is_empty() && self.process_close.idle() {
            self.process_close.abandon_quit();
            return true;
        }
        log!(
            "wm: quit waits: instances {refused:?} ask, apps {:?} are answering",
            ask.asked
        );
        self.quit_waiting_since = Some(now);
        if let Some(&first) = refused.first() {
            self.activate_client(cx, first);
        }
        self.update_bar(cx);
        self.redraw_all(cx);
        false
    }

    /// A client the pool is holding: no tile, no focus, invisible.
    fn is_warm(&mut self, client: ClientId) -> bool {
        self.state_mut()
            .clients
            .get(&client)
            .map(|s| s.warm)
            .unwrap_or(false)
    }

    /// A process app died unexpectedly: everything behind its tile goes
    /// (the bus's registrations, its peer link's calls and contexts, the
    /// process handle) and the tile stays, closed with a Restart.
    fn process_stopped(&mut self, cx: &mut Cx, client: ClientId) {
        let Some(slot) = self.state_mut().clients.get_mut(&client) else { return };
        slot.stopped = true;
        slot.sender = None;
        slot.socket = None;
        if let Some(mut child) = slot.child.take() {
            let _ = child.wait();
        }
        let app = slot.app.clone();
        let label = clients::find_app(&app).map(|a| a.label).unwrap_or_else(|| app.clone());
        log!("wm: {app} (client {client}) stopped; its tile stays closed with Restart");
        peer_link::process_gone(client);
        if let Some(bye) = self.ai_bus.client_died(client) {
            self.send_to_pane(bye);
        }
        self.pane_links.close_instance(client);
        self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| d.with_run_view(cx, client, |cx, v| v.show_stopped(cx, client, &label)));
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    fn remove_client(&mut self, cx: &mut Cx, client: ClientId) {
        log!("wm: removing client {}", client);
        peer_link::process_gone(client);
        self.process_close.gone(client);
        // An instance's extra window: the tile lets go of the root (the
        // instance keeps the widget), and the instance hears it was closed.
        if let Some((owner, key)) = self.module_windows.remove(&client) {
            self.desk(cx)
                .borrow_mut::<WmDesk>()
                .map(|mut d| d.with_module_view(cx, client, |cx, v| v.clear_root(cx)));
            self.module_host.notify_window_closed(owner, key);
        }
        // An instance going away takes its extra windows with it, first.
        if self.module_host.is_module(client) {
            let windows: Vec<ClientId> = self.module_windows.iter()
                .filter(|(_, (owner, _))| *owner == client).map(|(w, _)| *w).collect();
            for window in windows {
                self.remove_client(cx, window);
            }
        }
        // A module instance: the tile lets go of the root FIRST, then the
        // instance and its isolate go (module_host.rs). The bus hears an
        // Unregister for it below, like for any client.
        if self.module_host.is_module(client) {
            self.desk(cx)
                .borrow_mut::<WmDesk>()
                .map(|mut d| d.with_module_view(cx, client, |cx, v| v.clear_root(cx)));
            self.module_host.teardown(cx, client);
        }
        // The bus: the pane's own death empties the slot (the next F10
        // launches afresh); anyone else's death is an `Unregister` the
        // pane hears on their behalf, so its tool table shrinks at once.
        if self.ai_bus.is_pane(client) {
            log!("wm: the AI pane's child died");
            self.with_ai_pane(cx, |cx, p| p.reset(cx));
        }
        if let Some(bye) = self.ai_bus.client_died(client) {
            self.send_to_pane(bye);
        }
        // An in-process assistant sees the instance's link close.
        self.pane_links.close_instance(client);
        // A dying warm instance clears its pool slot, so the next launch
        // of that app falls back to a cold spawn instead of talking to a
        // dead socket, and the pool heals itself. A death nobody asked for
        // is a crash and is counted: three of those a minute and the pool
        // gives that app up quietly rather than respawning forever.
        let asked_for_it = self
            .state_mut()
            .clients
            .get(&client)
            .map(|s| s.closing.is_some())
            .unwrap_or(false);
        if let Some(app) = self.warm_pool.forget(client) {
            if !asked_for_it {
                self.warm_pool.note_crash(&app, host::now());
                log!("wm: warm {} (client {}) died", app, client);
            }
            // The top-up runs on the tick — one spawn at a time, and it
            // will only happen at all if the crash budget allows it.
        }
        self.warm_frames.remove(&client);
        self.warm_last_tick.remove(&client);
        // A home tile's client is gone: the tile shows the launcher again
        // (within its relaunch budget) on the next phone frame.
        if let Some(app) = self.state_mut().phone.tiles.forget_client(client) {
            log!("wm: home tile {} lost client {}", app, client);
        }
        self.state_mut().layout.remove(client);
        self.state_mut().clients.remove(&client);
        hub::revoke_launch_token(client);
        // A dying warm viewer (or its requester) clears the cache's
        // reference to it — "if a viewer process dies clear its slot, so
        // the next Space respawns" instead of talking to a dead socket.
        self.preview_cache.forget_client(client);
        if let Some(mut desk) = self.desk(cx).borrow_mut::<WmDesk>() {
            desk.remove_client(client);
        }
        self.focus_after_layout(cx);
        self.redraw_all(cx);
    }

    pub fn activate_client(&mut self, cx: &mut Cx, client: ClientId) {
        // A client the home page launched for its tile becomes a real
        // window on its first open — the same client, seated in the layout.
        self.promote_tile_client(cx, client);
        if self.state_mut().style.target.mobile() {
            self.state_mut().phone.activate(client);
            // In front now: its full face and full viewport go out at once,
            // so the zoom-in never plays over a compact frame.
            self.sync_home_tiles(cx);
            self.animate_phone(cx);
        } else {
            self.replay_tile_face(cx, client);
        }
        self.focus_client(cx, client);
    }

    fn focus_client(&mut self, cx: &mut Cx, client: ClientId) {
        // The pane's child is not in the layout: it has its own focus path.
        if self.ai_bus.is_pane(client) {
            self.focus_pane(cx);
            return;
        }
        // FOCUS RULE: a Quick Look preview never takes key focus.
        let takes_focus = self
            .state_mut()
            .clients
            .get(&client)
            .map(|s| s.takes_focus)
            .unwrap_or(true);
        if !takes_focus {
            return;
        }
        // Defensive: the menu is modal and must never still read "open"
        // once a hosted client is about to take key focus — every launch
        // path already closes it first, but force it shut here too so a
        // stray leftover state can never hijack this client's keystrokes.
        {
            let menu = self.ui.widget(cx, ids!(shell_menu));
            let mut borrowed = menu.borrow_mut::<ShellMenu>();
            if let Some(m) = borrowed.as_mut() {
                if m.is_open() {
                    m.close(cx);
                }
            }
        }
        if let Some(ws) = self.state_mut().layout.workspace_of(client) {
            let state = self.state_mut();
            if ws != state.layout.active {
                state.layout.switch_workspace(ws);
            }
            state.layout.workspaces[ws].focus = Some(client);
            state.layout.note_focus(client);
            if state.layout.desktop.enabled {state.layout.raise_float(client);}
        }
        // The tile widget only exists once the desk has drawn it, and its
        // Area only after its first draw — a focus at launch time lands on
        // nothing. Keep it pending and re-assert when the child's first
        // frame arrives (the PresentableDraw path below).
        if let Some(previous) = self.keyboard_holder.filter(|previous| *previous != client) {
            let _ = self
                .desk(cx)
                .borrow_mut::<WmDesk>()
                .and_then(|mut d| d.with_tile(cx, previous, |cx, v| v.release_keyboard(cx)));
            self.keyboard_holder = None;
        }
        let focused = self
            .desk(cx)
            .borrow_mut::<WmDesk>()
            .and_then(|mut d| d.with_tile(cx, client, |cx, v| v.focus_keyboard(cx)))
            .unwrap_or(false);
        self.pending_focus = if focused { None } else { Some(client) };
        if focused {
            self.keyboard_holder = Some(client);
        }
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// A freshly linked binary stalls on its first exec while macOS scans
    /// it (XprotectService/syspolicyd; the second exec is instant). Say so
    /// instead of leaving the tile silent.
    fn explain_first_exec_scan(&mut self, cx: &mut Cx) {
        const GRACE: f64 = 3.0;
        let waiting: Vec<ClientId> = self
            .state_mut()
            .clients
            .iter()
            .filter(|(_, s)| {
                // Warm instances excluded: nobody is watching a tile that
                // does not exist, and asking for one would build it.
                !s.warm
                    && !s.stopped
                    && s.linked
                    && s.sender.is_none()
                    && s.linked_at.map(|t| host::now() - t > GRACE).unwrap_or(false)
            })
            .map(|(id, _)| *id)
            .collect();
        for client in waiting {
            let text = "macOS is verifying the new binary…".to_string();
            let mut pane = false;
            if let Some(slot) = self.state_mut().clients.get_mut(&client) {
                if slot.status == text {
                    continue;
                }
                slot.status = text.clone();
                slot.linked = false;
                pane = slot.pane;
            }
            // Asking the desk for the pane child's tile would BUILD one.
            if pane {
                self.with_pane_run_view(cx, |cx, v| v.set_status_line(cx, &text));
                continue;
            }
            self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
                d.with_run_view(cx, client, |cx, v| v.set_status_line(cx, &text))
            });
        }
    }

    fn reap_exited(&mut self, cx: &mut Cx) {
        // Capture diagnostics queued by the output pumps before removing a
        // failed Cargo launch and its temporary progress tile.
        self.drain_client_lines(cx);
        // A process app that promised to answer a close and did not: the
        // fallback ends it (process_close.rs). One asking the person is
        // never among these.
        for client in self.process_close.overdue(host::now()) {
            log!(
                "wm: client {client} did not answer its close in {}s; ending it",
                process_close::ANSWER_TIMEOUT
            );
            self.end_process_client(cx, client);
        }
        // A quit nobody answered for a minute: the person kept the asking
        // app (it does not say so). Drop the quit and say so, so a later
        // yes or exit of that app does not quit the shell.
        let quit_waiting = self.process_close.quit_waiting() || self.module_host.close_gate_mut().quit_waiting();
        if !quit_waiting {
            self.quit_waiting_since = None;
        } else if process_close::quit_expired(self.quit_waiting_since, quit_waiting, host::now()) {
            self.quit_waiting_since = None;
            self.process_close.abandon_quit();
            self.module_host.abandon_quit();
            log!("wm: the quit waited {}s on an app asking the person; dropped", process_close::QUIT_ANSWER_WINDOW);
            self.notify(cx, "Quit cancelled", "An app asked before closing and was kept open. Quit again when you are ready.");
        }
        // A client that ignored the polite close gets the fallback.
        let pool = cx.task_pool();
        for slot in self.state_mut().clients.values_mut() {
            let Some(since) = slot.closing else { continue };
            if host::now() - since < clients::CLOSE_GRACE.as_secs_f64() {
                continue;
            }
            if let Some(child) = slot.child.as_mut() {
                if matches!(child.try_wait(), Ok(None)) {
                    clients::kill_child_group(child, clients::GROUP_KILL_GRACE, &pool);
                }
            }
        }
        // A launch whose build finished starts its app now (or fails; the
        // reaping below reports it). Launches closed while building start
        // nothing.
        let started: Vec<(ClientId, Result<(), String>)> = self
            .state_mut()
            .clients
            .iter_mut()
            .filter_map(|(id, slot)| slot.continue_launch(*id).map(|r| (*id, r)))
            .collect();
        for (id, result) in started {
            match result {
                Ok(()) => log!("wm: client {id}: built; the app starts"),
                Err(why) => log!("wm: client {id}: {why}"),
            }
        }
        // An app that quits while a close of it is pending gave its answer
        // (yes): a close, not a crash.
        let mut answering: Vec<ClientId> = self.state_mut().clients.keys().copied().collect();
        answering.retain(|client| self.process_close.is_pending(*client));
        let now = host::now();
        let dead: Vec<(ClientId, Option<String>)> = self
            .state_mut()
            .clients
            .iter_mut()
            .filter_map(|(id, slot)| {
                // A build whose app has not started is not the app's exit
                // (`continue_launch` above takes it, next tick at the latest).
                if slot.building() {
                    return None;
                }
                let status = slot.child.as_mut()?.try_wait().ok()??;
                if answering.contains(id) && slot.closing.is_none() {
                    slot.closing = Some(now);
                }
                let label = clients::find_app(&slot.app)
                    .map(|app| app.label).unwrap_or_else(|| slot.app.clone());
                Some((*id, slot.exit_failure(status, &label)))
            })
            .collect();
        for (id, failure) in dead {
            // ADR 0004 §2: a process app that dies takes only itself down;
            // its tile stays, closed with a Restart.
            let in_place = self.state_mut().clients.get(&id).is_some_and(|s| s.stops_in_place(failure.is_some()));
            if in_place {
                self.process_stopped(cx, id);
            } else {
                self.remove_client(cx, id);
            }
            if let Some(message) = failure {
                log!("octosense: {message}");
                self.notify(cx, "App stopped", &message);
            }
        }
    }

    // --------------------------------------------------------------
    // The AI pane and its bus
    //
    // The chat is a CHILD (apps/aichat), seated in the pane slot instead
    // of a tile. The WM gives it three things and nothing more: the slot
    // (shell/ai_pane.rs), the bus — every other client's service frames,
    // stamped and routed (ai_bus.rs) — and its own `os` service on that
    // bus. The child is never in the layout, so nothing the desk
    // enumerates (alt-tab, the bar, workspace counts) can see it.
    // --------------------------------------------------------------

    fn with_ai_pane<R>(
        &mut self,
        cx: &mut Cx,
        f: impl FnOnce(&mut Cx, &mut ShellAiPane) -> R,
    ) -> Option<R> {
        let pane = self.ui.widget(cx, ids!(shell_ai_pane));
        let mut borrowed = pane.borrow_mut::<ShellAiPane>()?;
        Some(f(cx, &mut borrowed))
    }

    fn with_pane_run_view<R>(
        &mut self,
        cx: &mut Cx,
        f: impl FnOnce(&mut Cx, &mut MpRunView) -> R,
    ) -> Option<R> {
        self.with_ai_pane(cx, |cx, p| p.with_run_view(cx, f)).flatten()
    }

    fn ai_pane_is_open(&mut self, cx: &mut Cx) -> bool {
        self.with_ai_pane(cx, |_, p| p.is_open()).unwrap_or(false)
    }

    /// The desk ROW the pane and the desk share: its last drawn rect, or
    /// the window's startup proportions before the first draw. The desk
    /// also learns whether the pane is mid-slide, so its tiles snap to the
    /// narrowing layout every frame instead of tweening after it.
    fn sync_ai_pane_geometry(&mut self, cx: &mut Cx) {
        let row = self.ui.view(cx, ids!(desk_row)).area();
        let rect = if row.is_valid(cx) { row.rect(cx) } else { Rect::default() };
        let rect = if rect.size.x > 1.0 {
            rect
        } else {
            Rect {
                pos: dvec2(0.0, BAR_HEIGHT_FALLBACK),
                size: dvec2(1400.0, 860.0),
            }
        };
        let gap = self.state_mut().gaps_out;
        let sliding = self
            .with_ai_pane(cx, |_, p| {
                p.set_geometry(rect, gap);
                p.is_sliding()
            })
            .unwrap_or(false);
        self.state_mut().pane_sliding = sliding;
    }

    /// F10 / `--test-action ai`.
    fn toggle_ai_pane(&mut self, cx: &mut Cx) {
        if self.ai_pane_is_open(cx) {
            self.close_ai_pane(cx);
        } else {
            self.open_ai_pane(cx);
        }
    }

    /// Slide the pane in, launching the aichat child the first time (or
    /// again after it died). The keyboard goes to the child as soon as it
    /// has a frame to receive it in; every registered app hears
    /// `ChatOpen`.
    fn open_ai_pane(&mut self, cx: &mut Cx) {
        if !self.apps.pane_in_process() && !clients::find_app("aichat").is_some_and(|a| a.is_available()) {
            self.notify(cx, "Assistant unavailable", "Add a compatible assistant app to enable this feature.");
            return;
        }
        if self.apps.pane_in_process() {
            if !self.ensure_local_pane(cx) {
                return;
            }
        } else if self.ai_bus.pane_client.is_none() && !self.launch_ai_pane(cx) {
            return;
        }
        self.sync_ai_pane_geometry(cx);
        self.with_ai_pane(cx, |cx, p| p.set_open(cx, true));
        self.focus_pane(cx);
        self.broadcast_chat_open(cx, true);
        self.redraw_all(cx);
    }

    /// Slide the pane out; the child keeps running behind it. The
    /// keyboard returns to the layout's focused tile.
    fn close_ai_pane(&mut self, cx: &mut Cx) {
        if !self.ai_pane_is_open(cx) {
            return;
        }
        self.with_ai_pane(cx, |cx, p| p.set_open(cx, false));
        self.focus_after_layout(cx);
        self.broadcast_chat_open(cx, false);
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// Spawn the aichat child as the pane's client: the ordinary launch
    /// path (cargo, env, per-client log) minus the layout — `launch_app`
    /// is never used for it, since launch-or-focus would "focus" a pane.
    fn launch_ai_pane(&mut self, cx: &mut Cx) -> bool {
        let Some(app) = clients::find_app("aichat") else {
            log!("wm: no aichat entry in the registry");
            return false;
        };
        let hub_port = self.state_mut().hub_port;
        if hub_port == 0 {
            log!("wm: no hub; the AI pane cannot host a child");
            return false;
        }
        let id = self.next_id;
        self.next_id += 1;
        let lines = self.line_sender();
        let pool = cx.task_pool();
        match spawn_client(&pool, &cx.thread_spawner(), &app, id, hub_port, None, None, &[], false, lines) {
            Ok(mut slot) => {
                slot.pane = true;
                self.state_mut().clients.insert(id, slot);
                self.ai_bus.pane_client = Some(id);
                self.with_ai_pane(cx, |cx, p| {
                    p.set_client(Some(id));
                    p.with_run_view(cx, |cx, v| v.set_run_target(cx, id, 0, hub_port));
                });
                log!("wm: launched aichat as client {} (the AI pane)", id);
                true
            }
            Err(err) => {
                log!("wm: launching the AI pane failed: {}", err);
                false
            }
        }
    }

    /// The pane's keyboard goes to the child's run view; the pane itself
    /// keeps the request pending until its run view has drawn (see
    /// `ShellAiPane::focus_keyboard`).
    fn focus_pane(&mut self, cx: &mut Cx) {
        let local = self.with_ai_pane(cx, |_, p| p.is_local()).unwrap_or(false);
        if self.ai_bus.pane_client.is_none() && !local {
            return;
        }
        self.with_ai_pane(cx, |cx, p| p.focus_keyboard(cx));
    }

    /// A pointer event inside the OPEN pane is the pane's alone — routed
    /// to it directly, the way an open menu or flyout takes the pointer,
    /// so the tile underneath and the WM's own gestures never see it. A
    /// drag already in flight keeps its events: a window carried across
    /// the pane must still land.
    fn ai_pane_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let abs = match event {
            Event::MouseMove(e) => e.abs,
            Event::MouseDown(e) => e.abs,
            Event::MouseUp(e) => e.abs,
            Event::Scroll(e) => e.abs,
            _ => return false,
        };
        if self.drag.is_some() || self.div_drag.is_some() {
            return false;
        }
        let inside = self
            .with_ai_pane(cx, |_, p| p.contains(abs))
            .unwrap_or(false);
        if !inside {
            return false;
        }
        let pane = self.ui.widget(cx, ids!(shell_ai_pane));
        pane.handle_event(cx, event, &mut Scope::empty());
        true
    }

    /// One `Custom` frame to a connected client; false when it has no
    /// socket yet (or is gone).
    fn send_custom(&mut self, client: ClientId, json: String) -> bool {
        if let Some(slot) = self.state_mut().clients.get(&client) {
            if let Some(sender) = &slot.sender {
                send_to_app(sender, vec![StudioToApp::Custom(json)]);
                return true;
            }
        }
        false
    }

    /// A frame for the pane child. Dropped when the pane is not up: the
    /// bus already holds every registration for the replay, and a
    /// result nobody is waiting for has no one to go to.
    fn send_to_pane(&mut self, json: String) -> bool {
        match self.ai_bus.pane_client {
            Some(client) => self.send_custom(client, json),
            None => false,
        }
    }

    fn broadcast_chat_open(&mut self, cx: &mut Cx, open: bool) {
        for (client, json) in self.ai_bus.chat_open_frames(open) {
            self.send_custom(client, json);
        }
        // In-process instances hear it from the host directly.
        self.module_host.chat_open(cx, open);
    }

    /// The registry as the `os` brief names it: (id, label).
    fn registry_apps() -> Vec<(String, String)> {
        clients::available_apps()
            .iter()
            .filter(|a| apps::listed(&a.id))
            .map(|a| (a.id.clone(), a.label.clone()))
            .collect()
    }

    /// The pane connected: the WM's own registration, then every client's
    /// last one, in one batch.
    fn replay_bus_to_pane(&mut self) {
        let Some(client) = self.ai_bus.pane_client else {
            return;
        };
        let frames = self
            .ai_bus
            .replay(AiBus::os_manifest(&Self::registry_apps()));
        let sender = self
            .state_mut()
            .clients
            .get(&client)
            .and_then(|s| s.sender.clone());
        if let Some(sender) = sender {
            log!("wm: replaying {} registrations to the AI pane", frames.len());
            send_to_app(
                &sender,
                frames.into_iter().map(StudioToApp::Custom).collect(),
            );
        }
    }

    /// The pane child's own window requests. Its `Close` (Esc on an idle,
    /// empty composer) means HIDE — the process stays for the next F10;
    /// the rest of the window vocabulary has no window to apply to.
    fn on_pane_request(&mut self, cx: &mut Cx, req: WmRequest) {
        match req {
            WmRequest::Close => self.close_ai_pane(cx),
            WmRequest::Title { .. } | WmRequest::Cwd { .. } => {}
            other => log!(
                "wm: the AI pane asked for {:?}; a pane has no window for that",
                other
            ),
        }
    }

    fn on_bus_route(&mut self, cx: &mut Cx, route: Route) {
        match route {
            Route::ToPane(json) => {
                self.send_to_pane(json);
            }
            Route::ToClient(client, json) => {
                self.send_custom(client, json);
            }
            Route::Os(call) => {
                let result = self.answer_os_call(cx, &call);
                self.send_to_pane(AiBus::os_reply(result));
            }
            Route::Local(client, msg) => self.on_local_frame(cx, client, msg),
            // A `confirm: host` tool (the Terminal's `run`): the approval
            // router answers, now (developer mode, a rule) or from its sheet.
            Route::Approval(held) => {
                approvals::bus_requested(&held);
                self.approvals_changed(cx);
            }
            Route::ShellResult(result) => {
                host_tools::bus_result(&result.call_id, host_tools::outcome_of(&result));
                host_tools::pump();
            }
            Route::Drop => {}
        }
    }

    /// The WM is going down: the pane's child has no window of its own
    /// and no tile, so nothing else would reap it.
    fn kill_pane_child(&mut self, cx: &mut Cx) {
        let Some(client) = self.ai_bus.pane_client else {
            return;
        };
        let pool = cx.task_pool();
        if let Some(slot) = self.state_mut().clients.get_mut(&client) {
            if let Some(child) = slot.child.as_mut() {
                log!("wm: shutdown kills the AI pane's child {}", client);
                clients::kill_child_group(child, clients::GROUP_KILL_GRACE, &pool);
            }
        }
    }

    // ---- in-process instances (module_host.rs) ----

    /// Open `module` as an instance of its own in this process: an isolate,
    /// a tile in the layout, a local endpoint on the bus. The ordinary
    /// launch path minus everything a process needs.
    pub fn launch_module_as(&mut self, cx: &mut Cx, module: &'static dyn AppModule, app: &clients::AppDef) {
        let open = match apps::module_open(module, app) {
            Ok(open) => open,
            Err(e) => {
                log!("wm: {} cannot open without arguments: {}", module.id(), e);
                return;
            }
        };
        let id = self.next_id;
        self.next_id += 1;
        let area = self.desk_area(cx);
        // Extra windows are a desktop thing; the phone shell's apps are full-screen.
        self.module_host.extra_windows = !self.state_mut().style.target.mobile();
        if let Err(e) = self.module_host.create(cx, id, module, open, dvec2(area.w, area.h)) {
            log!("wm: module {} failed to start: {}", module.id(), e);
            return;
        }
        self.modules_launched += 1;
        let (manifest, root, vm_id) = match self.module_host.get(id) {
            Some(instance) => (instance.manifest(), instance.root.clone(), instance.vm_id),
            None => return,
        };
        self.state_mut()
            .clients
            .insert(id, clients::ClientSlot::module(id, &app.id, &app.label));
        let gap = self.state_mut().gap;
        self.state_mut().layout.insert(id, area, gap);
        // The tile is a module tile from its first draw; the root is seated
        // in it before anything asks it to draw.
        self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
            d.mark_module(id);
            d.with_module_view(cx, id, |cx, v| v.set_root(cx, id, vm_id, root));
        });
        // On the bus. An in-process assistant adopts the instance as a
        // link (waiting on `Cx` until the pane's root exists); the aichat
        // child learns of it as a local endpoint now, or at the replay
        // when it connects later.
        if self.apps.pane_in_process() {
            self.pane_links.open_instance(cx, id, manifest);
        } else {
            let frame = self.ai_bus.register_local(id, manifest);
            self.send_to_pane(frame);
        }
        log!("wm: launched {} as client {} (in-process, {})", app.id, id, module.id());
        self.activate_client(cx, id);
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// The instances whose module panicked since the last event
    /// (module_host.rs, "PANIC CONTAINMENT"): their in-flight tool calls
    /// are answered (outcome unknown where they may have acted), their
    /// extra windows close,
    /// the tile lets go of the root and shows the app closed with a
    /// Restart, the assistant loses its tools, and the instance's
    /// resources are freed. The shell and every other app go on.
    pub(crate) fn contain_module_faults(&mut self, cx: &mut Cx) {
        let failed = self.module_host.take_faults(cx);
        if !failed.is_empty() {
            // The failed instances' in-flight calls are answered now —
            // outcome unknown where they may have acted (module_host.rs,
            // `outcome_unknown`) — while their endpoints still exist.
            self.drain_module_upstream();
        }
        for (client, label) in failed {
            let windows: Vec<ClientId> = self.module_windows.iter()
                .filter(|(_, (owner, _))| *owner == client).map(|(w, _)| *w).collect();
            for window in windows {
                self.remove_client(cx, window);
            }
            self.desk(cx)
                .borrow_mut::<WmDesk>()
                .map(|mut d| d.with_module_view(cx, client, |cx, v| v.show_failed(cx, &label)));
            self.module_host.release_failed(cx, client);
            if let Some(bye) = self.ai_bus.client_died(client) {
                self.send_to_pane(bye);
            }
            self.pane_links.close_instance(client);
            self.update_bar(cx);
            self.redraw_all(cx);
        }
    }

    /// Restart on a failed module's tile: that window closes and the app
    /// opens afresh, as a launch from the menu would open it.
    fn restart_failed_module(&mut self, cx: &mut Cx, client: ClientId) {
        let stopped = self.state.as_ref().and_then(|s| s.clients.get(&client)).is_some_and(|s| s.stopped);
        if !self.module_host.is_failed(client) && !stopped {
            return;
        }
        let Some(app) = self.state.as_ref().and_then(|s| s.clients.get(&client)).map(|slot| slot.app.clone()) else {
            return;
        };
        log!("wm: restarting {app} after its instance (client {client}) failed");
        self.request_close(cx, client);
        self.launch_app(cx, &app);
    }

    /// A frame the pane addressed to an in-process instance.
    fn on_local_frame(&mut self, cx: &mut Cx, client: ClientId, msg: ServiceDown) {
        match msg {
            ServiceDown::Call(call) => {
                let result = match self.module_host.execute(cx, client, &call) {
                    Some(ExecOutcome::Done(result)) => result,
                    // Answered later through the instance's reply sink.
                    Some(ExecOutcome::Pending) => return,
                    None => ToolResult::unavailable(&call.call_id, "that app is gone"),
                };
                if self.ai_bus.take_shell_result(client, &result.call_id) {
                    host_tools::bus_result(&result.call_id, host_tools::outcome_of(&result));
                    host_tools::pump();
                    return;
                }
                let frame = self.ai_bus.local_reply(client, result);
                self.send_to_pane(frame);
            }
            ServiceDown::Cancel { call_id } => self.module_host.cancel(cx, client, &call_id),
            ServiceDown::Subscribe { sub_id, topic, filter } => {
                self.module_host.subscribe(cx, client, &sub_id, &topic, filter.as_deref())
            }
            ServiceDown::Unsubscribe { sub_id } => self.module_host.unsubscribe(cx, client, &sub_id),
            // The pane state reaches instances through `broadcast_chat_open`.
            ServiceDown::ChatOpen { .. } | ServiceDown::Registered { .. } => {}
        }
    }

    /// Results in-process executors answered later, up to the pane: down
    /// the instance's link when the assistant is in-process, as a frame to
    /// the aichat child otherwise.
    fn drain_module_upstream(&mut self) {
        for (client, upstream) in self.module_host.drain_upstream() {
            match upstream {
                ModuleUpstream::Result(result) => {
                    if self.ai_bus.take_shell_result(client, &result.call_id) {
                        host_tools::bus_result(&result.call_id, host_tools::outcome_of(&result));
                        host_tools::pump();
                    } else if self.pane_links.is_instance(client) {
                        self.pane_links.reply(client, result);
                    } else {
                        let frame = self.ai_bus.local_reply(client, result);
                        self.send_to_pane(frame);
                    }
                }
                ModuleUpstream::Message { sub_id, message } => {
                    if self.pane_links.is_instance(client) {
                        self.pane_links.publish(client, sub_id, message);
                    } else {
                        let frame = self.ai_bus.local_message(client, sub_id, message);
                        self.send_to_pane(frame);
                    }
                }
            }
        }
    }

    // ---- the assistant in-process (pane_links.rs) ----

    /// The pane's body is the aichat module: seat it (by name), and put
    /// the WM's own `os` service on its registry. False, logged, when this
    /// build does not link the assistant.
    fn ensure_local_pane(&mut self, cx: &mut Cx) -> bool {
        let seated = self.with_ai_pane(cx, |cx, p| p.ensure_overlay(cx)).unwrap_or(false);
        if !seated {
            return false;
        }
        self.pane_links.open_os(cx, AiBus::os_manifest(&Self::registry_apps()));
        true
    }

    /// What the in-process assistant asked through its links: the `os`
    /// calls are the WM's, the rest go to the instances' executors.
    fn drain_pane_links(&mut self, cx: &mut Cx) {
        for call in self.pane_links.drain() {
            match call {
                PaneCall::Os(call) => {
                    let result = self.answer_os_call(cx, &call);
                    self.pane_links.reply_os(result);
                }
                PaneCall::Instance(client, call) => {
                    let result = match self.module_host.execute(cx, client, &call) {
                        Some(ExecOutcome::Done(result)) => result,
                        // Answered later through the instance's reply sink.
                        Some(ExecOutcome::Pending) => continue,
                        None => ToolResult::unavailable(&call.call_id, "that app is gone"),
                    };
                    self.pane_links.reply(client, result);
                }
                PaneCall::Cancel(client, call_id) => self.module_host.cancel(cx, client, &call_id),
                PaneCall::Subscribe { client, sub_id, topic, filter } => {
                    self.module_host.subscribe(cx, client, &sub_id, &topic, filter.as_deref())
                }
                PaneCall::Unsubscribe { client, sub_id } => {
                    self.module_host.unsubscribe(cx, client, &sub_id)
                }
            }
        }
        // The chat's own Escape (an idle, empty composer) asks the slot it
        // would sit in to close; in the pane that slot is us.
        if cx.global::<AiSlotRequests>().open.take() == Some(false) {
            self.close_ai_pane(cx);
        }
    }

    // ---- the `os` service ----

    /// Every app the desktop knows, with its window state: the registry
    /// rows first, then any window of an app outside the menu (a viewer
    /// opened on a file).
    /// Instances' extra-window requests (`ModuleWindows`): each root becomes
    /// a module tile of its own, drawn and fed events in the owner's isolate.
    fn drain_module_windows(&mut self, cx: &mut Cx) {
        for (owner, vm_id, app, request) in self.module_host.take_window_requests() {
            let existing = |this: &Self, key: LiveId| this.module_windows.iter()
                .find(|(_, (o, k))| *o == owner && *k == key).map(|(w, _)| *w);
            match request {
                WindowRequest::Open { key, title, root, size: _ } => {
                    if let Some(window) = existing(self, key) {
                        self.activate_client(cx, window);
                        continue;
                    }
                    let id = self.next_id;
                    self.next_id += 1;
                    let area = self.desk_area(cx);
                    self.state_mut().clients.insert(id, clients::ClientSlot::module(id, app, &title));
                    let gap = self.state_mut().gap;
                    self.state_mut().layout.insert(id, area, gap);
                    self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
                        d.mark_module(id);
                        d.with_module_view(cx, id, |cx, v| v.set_root(cx, id, vm_id, root));
                    });
                    self.module_windows.insert(id, (owner, key));
                    log!("wm: {} (client {}) opened window {:?} as client {}", app, owner, title, id);
                    self.activate_client(cx, id);
                    self.update_bar(cx);
                    self.redraw_all(cx);
                }
                WindowRequest::Close { key } => {
                    // The instance closed it itself: no report back, or a
                    // stale "closed" could end a window it reopens later.
                    if let Some(window) = existing(self, key) {
                        self.module_windows.remove(&window);
                        self.desk(cx)
                            .borrow_mut::<WmDesk>()
                            .map(|mut d| d.with_module_view(cx, window, |cx, v| v.clear_root(cx)));
                        self.remove_client(cx, window);
                        self.update_bar(cx);
                    }
                }
            }
        }
    }

    fn app_rows(&mut self) -> Vec<OsAppRow> {
        let state = self.state_mut();
        let focused = state.layout.focused_client();
        let mut rows: Vec<OsAppRow> = clients::available_apps()
            .iter()
            .filter(|a| apps::listed(&a.id))
            .map(|a| OsAppRow {
                id: a.id.clone(),
                label: a.label.clone(),
                running: false,
                focused: false,
            })
            .collect();
        let mut windows: Vec<(ClientId, String)> = state
            .clients
            .iter()
            .filter(|(id, s)| {
                !s.warm && !s.pane && s.closing.is_none() && state.layout.workspace_of(**id).is_some()
            })
            .map(|(id, s)| (*id, s.app.clone()))
            .collect();
        windows.sort_unstable();
        for (id, app) in windows {
            if !rows.iter().any(|r| r.id == app) {
                rows.push(OsAppRow {
                    id: app.clone(),
                    label: clients::find_app(&app)
                        .map(|a| a.label)
                        .unwrap_or_else(|| app.clone()),
                    running: false,
                    focused: false,
                });
            }
            let row = rows.iter_mut().find(|r| r.id == app).unwrap();
            row.running = true;
            if focused == Some(id) {
                row.focused = true;
            }
        }
        rows
    }

    /// The oldest window of `app` (launch-or-focus's `head -n1` rule).
    fn client_for_app(&mut self, app: &str) -> Option<ClientId> {
        let state = self.state_mut();
        let mut ids: Vec<ClientId> = state
            .clients
            .iter()
            .filter(|(id, s)| {
                s.app == app
                    && !s.warm
                    && !s.pane
                    && s.closing.is_none()
                    && state.layout.workspace_of(**id).is_some()
            })
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        ids.first().copied()
    }

    /// The WM as a service. Every branch answers; every answer is bounded.
    fn answer_os_call(&mut self, cx: &mut Cx, call: &ServiceCall) -> ToolResult {
        let id = call.call_id.as_str();
        let mut result = match call.tool.as_str() {
            "list" => os_list_result(id, &self.app_rows()),
            "launch" => match AiBus::app_arg(call) {
                None => ToolResult::refused(id, "launch needs an `app` id from os.list"),
                Some(app) if app == "aichat" => {
                    ToolResult::ok(id, "the assistant is already open", "already open")
                }
                Some(app) => match clients::find_app(&app) {
                    None => ToolResult::refused(
                        id,
                        format!("no app `{app}`; known: {}", known_app_ids()),
                    ),
                    Some(def) if !host::processes_available() && self.apps.hosting(&def.id) != Hosting::Module => {
                        ToolResult::unavailable(
                            id,
                            format!(
                                "{} is not part of this build; it links {}",
                                def.label,
                                self.apps.linked_ids().join(", ")
                            ),
                        )
                    }
                    Some(def) => {
                        // Already up: say so plainly, so the model calls the
                        // app's tools instead of waiting for it to appear.
                        let running = def.policy == LaunchPolicy::OrFocus && self.client_for_app(&def.id).is_some();
                        if let (true, Some(client)) = (running, self.client_for_app(&def.id)) {
                            self.focus_client(cx, client);
                        } else {
                            self.launch_app(cx, &def.id);
                        }
                        // The person is talking to the chat: the new
                        // window takes the layout's focus, not their keys
                        // — including the focus a tile that has not drawn
                        // yet would otherwise claim on its first frame.
                        self.pending_focus = None;
                        self.focus_pane(cx);
                        os_launch_answer(id, &def.label, running)
                    }
                },
            },
            "focus" | "close" => match AiBus::app_arg(call) {
                None => ToolResult::refused(id, format!("{} needs an `app` id", call.tool)),
                Some(app) => match self.client_for_app(&app) {
                    None => ToolResult::failed(id, format!("{app} is not running")),
                    Some(client) if call.tool == "focus" => {
                        self.focus_client(cx, client);
                        self.pending_focus = None;
                        self.focus_pane(cx);
                        ToolResult::ok(id, format!("{app} is in front"), "focused")
                    }
                    Some(client) => {
                        self.request_close(cx, client);
                        if self.close_pending(client) {
                            ToolResult::ok(id, format!("{app} was asked to close and may ask the person first"), "asking")
                        } else {
                            ToolResult::ok(id, format!("{app} is closing"), "closing")
                        }
                    }
                },
            },
            "open" => match AiBus::str_arg(call, "path") {
                None => ToolResult::refused(id, "open needs a `path`"),
                Some(path) => {
                    let req = WmRequest::Open {
                        app: AiBus::app_arg(call),
                        path: path.clone(),
                    };
                    match preview::OpenRequest::from_request(&req) {
                        Some(open) if clients::find_app(&open.app).is_some() => {
                            let app = open.app.clone();
                            self.open_request(cx, None, open);
                            ToolResult::ok(id, format!("opening {path} in {app}"), "opening")
                        }
                        _ => ToolResult::refused(id, format!("no app can open {path}")),
                    }
                }
            },
            other => ToolResult::refused(
                id,
                format!("os has no tool `{other}`; it has list, launch, focus, close, open"),
            ),
        };
        result.bound();
        result
    }

    // --------------------------------------------------------------
    // Hub routing
    // --------------------------------------------------------------

    fn drain_hub(&mut self, cx: &mut Cx) {
        let Some(hub) = self.hub.as_ref() else {
            return;
        };
        let mut events = Vec::new();
        while let Ok(event) = hub.rx.try_recv() {
            events.push(event);
        }
        for event in events {
            match event {
                HubEvent::Connected {
                    client,
                    socket,
                    sender,
                } => {
                    // The hub admitted this socket with the launch's own
                    // secret; the slot must still be that live process,
                    // and not bound already (ADR 0004 §5).
                    if !slot_accepts_socket(self.state_mut().clients.get(&client)) {
                        log!("wm: refused a socket for client {client}: no unbound process slot");
                        let _ = sender.send(Vec::new());
                        continue;
                    }
                    let theme_splash = theme::theme_splash_path(&self.state_mut().theme_name)
                        .to_string_lossy()
                        .to_string();
                    if let Some(slot) = self.state_mut().clients.get_mut(&client) {
                        // The peer link (ADR 0004 §5): the socket is this
                        // slot's app; granted apps get a link on it.
                        peer_link::connected(client, &slot.app, sender.clone());
                        slot.sender = Some(sender);
                        slot.socket = Some(socket);
                        if let Some(sender) = &slot.sender {
                            send_to_app(
                                sender,
                                vec![StudioToApp::Custom(
                                    WmEvent::Hosted { theme_splash }.to_json(),
                                )],
                            );
                        }
                    }
                    self.send_desktop_style(client);
                    // The pane just connected (or reconnected): it learns
                    // every current registration, the WM's own first.
                    if self.ai_bus.is_pane(client) {
                        self.replay_bus_to_pane();
                    }
                    // A Quick-Look retarget that arrived while this viewer
                    // was still starting: it has a socket now, so land it.
                    if let Some(path) = self.preview_cache.take_pending(client) {
                        self.send_wm_event(client, WmEvent::PreviewFile { path });
                    }
                }
                HubEvent::Disconnected { socket } => {
                    let client = self
                        .state_mut()
                        .clients
                        .iter()
                        .find(|(_, s)| s.socket == Some(socket))
                        .map(|(id, _)| *id);
                    if let Some(client) = client {
                        peer_link::process_gone(client);
                        if let Some(slot) = self.state_mut().clients.get_mut(&client) {
                            slot.sender = None;
                            slot.socket = None;
                        }
                    }
                }
                HubEvent::FromApp { client, socket, msgs } => {
                    // Only the socket bound to a live slot speaks for it: a
                    // slot that is gone, or a socket that is not (or no
                    // longer) the bound one, is not heard.
                    if !frame_is_bound(self.state_mut().clients.get(&client), socket) {
                        continue;
                    }
                    for msg in msgs {
                        self.on_app_msg(cx, client, msg);
                    }
                }
            }
        }
    }

    fn on_app_msg(&mut self, cx: &mut Cx, client: ClientId, msg: AppToStudio) {
        match msg {
            AppToStudio::CreateWindow { window_id, .. } => {
                // A multi-window app (the VJ: console + output window)
                // announces every window; the tile hosts the FIRST — the
                // app's main UI — never a later output/aux window.
                let first = self
                    .state_mut()
                    .clients
                    .get_mut(&client)
                    .map(|slot| {
                        if slot.ready {
                            false
                        } else {
                            slot.window_id = window_id;
                            slot.ready = true;
                            true
                        }
                    })
                    .unwrap_or(false);
                if first {
                    // A warm instance has no tile to be ready IN: it gets
                    // a framebuffer of its own instead, and draws into it
                    // until someone adopts it. The pane's child is ready
                    // in the pane's own run view.
                    if self.is_warm(client) {
                        self.warm_bootstrap(cx, client, window_id);
                    } else if self.ai_bus.is_pane(client) {
                        self.with_pane_run_view(cx, |cx, v| v.app_ready(cx, client, window_id));
                    } else {
                        self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
                            d.with_run_view(cx, client, |cx, v| v.app_ready(cx, client, window_id))
                        });
                    }
                    self.send_desktop_style(client);
                    // A home tile's client hears which face to show before
                    // its first frame (mobile_app.rs).
                    self.replay_tile_face(cx, client);
                }
            }
            AppToStudio::DrawCompleteAndFlip(pd) => {
                crate::run_view::trace_host(&format!("rx-flip c{}", client));
                // A CLOSING client's tile is frozen on its last good frame:
                // the zoom-out plays over that, never over the app's own
                // shutdown relayout/clipping.
                let closing = self
                    .state_mut()
                    .clients
                    .get(&client)
                    .map(|s| s.closing.is_some())
                    .unwrap_or(false);
                if closing {
                    return;
                }
                // A dormant instance's frames land in its own off-desk
                // framebuffer; there is no tile to present them in, and
                // touching the desk here would build one. The first frame
                // is the moment it is warm all the way through.
                if self.is_warm(client) {
                    if let Some(frame) = self.warm_frames.get_mut(&client) {
                        if !frame.presented {
                            frame.presented = true;
                            log!("wm: warm client {} drew its first frame", client);
                        }
                    }
                    return;
                }
                if self.ai_bus.is_pane(client) {
                    self.with_pane_run_view(cx, |cx, v| v.set_presentable_draw(cx, pd));
                    self.note_first_frame(client);
                    return;
                }
                // Which of the phone's captures this frame may refresh: a
                // tile client's frames are sorted by size, so a card never
                // shows a stretched tile and a tile never a squeezed window.
                let face = self.note_client_frame_face(client, pd.width, pd.height);
                self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
                    d.note_client_frame(client, face);
                    d.with_run_view(cx, client, |cx, v| v.set_presentable_draw(cx, pd))
                });
                self.note_first_frame(client);
                // A focus that couldn't land at launch (tile not yet
                // drawn) lands now that the client has a frame.
                if self.pending_focus == Some(client) {
                    self.focus_client(cx, client);
                }
            }
            AppToStudio::TickDone => {
                // Dormant warm clients are pumped by the warm heartbeat.
                if self.is_warm(client) {
                    return;
                }
                if self.ai_bus.is_pane(client) {
                    self.with_pane_run_view(cx, |cx, v| v.tick_done(cx));
                } else {
                    self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
                        d.with_run_view(cx, client, |cx, v| v.tick_done(cx))
                    });
                }
            }
            AppToStudio::SetCursor(cursor) => {
                if self.ai_bus.is_pane(client) {
                    self.with_pane_run_view(cx, |cx, v| v.set_remote_cursor(cx, cursor.into()));
                    return;
                }
                self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
                    d.with_run_view(cx, client, |cx, v| v.set_remote_cursor(cx, cursor.into()))
                });
            }
            AppToStudio::SetClipboard(text) => {
                cx.copy_to_clipboard(&text);
            }
            AppToStudio::Custom(json) => {
                if let Some(back) = makepad_platform::ime::HostedBack::parse(&json) {
                    if back.handled == Some(false) && self.state_mut().phone.client == Some(client) {
                        self.state_mut().phone.navigate(mobile::PhoneScreen::Home);
                        self.animate_phone(cx);
                    }
                    return;
                }
                if let Some(ime) = makepad_platform::ime::HostedImeState::parse(&json) {
                    self.state_mut().phone.ime.insert(client,ime);
                    self.sync_phone_keyboard(cx);
                    return;
                }
                // The peer link's own channel (ADR 0004 §5): never the
                // bus's. Its app is the socket's slot, never the frame's.
                if peer_link::wire::is_peer_frame(&json) {
                    let (app, sender) = match self.state_mut().clients.get(&client) {
                        Some(slot) => (slot.app.clone(), slot.sender.clone()),
                        None => return,
                    };
                    peer_link::on_frame(client, &app, &json, sender);
                    return;
                }
                // An app's answer about closes, read by its wire text so it
                // works with any makepad pin (process_close.rs).
                if let Some(word) = process_close::parse_close_word(&json) {
                    self.on_close_word(cx, client, word);
                    return;
                }
                // The typed app<->WM vocabulary (libs/wm_api) first; what
                // is not the WM's own envelope is the AI bus's (or noise).
                if let Some(req) = WmRequest::parse(&json) {
                    if self.ai_bus.is_pane(client) {
                        self.on_pane_request(cx, req);
                    } else {
                        self.on_wm_request(cx, client, req);
                    }
                    return;
                }
                let app = self.state_mut().clients.get(&client).map(|slot| slot.app.clone());
                let route = self.ai_bus.on_custom_from(client, app.as_deref(), &json);
                for (to, confirm) in self.ai_bus.take_confirmations() {
                    self.send_custom(to, confirm);
                }
                self.on_bus_route(cx, route);
            }
            AppToStudio::LogItem(item) => {
                // Hosted children log over the studio socket, not stdout —
                // dropping these makes them undebuggable. Into our own log
                // (and thus the remote bridge's /log) they go.
                log!("client {}: {}", client, item.message);
            }
            _ => {}
        }
    }

    // --------------------------------------------------------------
    // Menu
    // --------------------------------------------------------------

    // --------------------------------------------------------------
    // Theme / background
    // --------------------------------------------------------------

    fn set_theme(&mut self, cx: &mut Cx, name: &str) {
        let state = self.state_mut();
        state.theme_name = name.to_string();
        let source = theme::load_theme_source(name);
        if let Some(palette) = theme::scan_term_palette(&source) {
            state.term_env = palette.env_value();
        }
        if let Some(rgb) = scan_theme_color(&source, "accent") {
            state.accent = rgb;
        }
        state.borders = desk::BorderTheme::from_theme_source(&source);
        // Children style themselves from the same file; the choice outlives
        // this run (a state file natively, the desk's storage on the web).
        host::set_child_env("MAKEPAD_WM_THEME_SPLASH", theme::theme_splash_path(name).as_os_str());
        host::persist_theme_choice(cx, name);
        let style = self.state_mut().style.target;
        if style == desktop::DesktopStyle::Omarchy {
            let dark = desktop_app::browser_appearance(style, false, &source);
            self.refresh_warm_browser_appearance(cx, dark);
        }
        // Chrome DSL colors refresh fully on restart; borders, terminal
        // palette and backgrounds apply immediately.
        self.apply_background(cx, 0);
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// A theme with no pictures yet fetches them from the omarchy repo on a
    /// thread of its own; the desk shows the first one when it lands (the
    /// tick polls `theme_backgrounds` while `backgrounds_pending`). This is
    /// opt-in; the bundled default wallpaper already works offline.
    fn fetch_backgrounds_if_missing(&mut self, cx: &mut Cx) {
        if !host::processes_available() {
            return;
        }
        let name = self.state_mut().theme_name.clone();
        if !theme::theme_backgrounds(&name).is_empty() {
            return;
        }
        self.backgrounds_pending = true;
        log!("wm: theme '{}' has no wallpapers; fetching them", name);
        match cx.task_pool().submit(Lane::Heavy, move || {
                let n = theme::fetch_backgrounds_if_missing(&name);
                log!("wm: fetched {} wallpaper(s) for '{}'", n, name);
                n
            }) {
            Ok(task) => self.background_task = Some(task),
            Err(error) => {
                self.backgrounds_pending = false;
                log!("wm: could not queue wallpaper fetch: {error}");
            }
        }
    }

    fn poll_backgrounds(&mut self, cx: &mut Cx) {
        let Some(task) = self.background_task.as_mut() else {
            return;
        };
        let Some(result) = task.try_take() else {
            return;
        };
        self.background_task = None;
        self.backgrounds_pending = false;
        match result {
            Ok(count) if count > 0 => { self.apply_background(cx, 0); }
            Ok(_) => {}
            Err(error) => log!("wm: wallpaper fetch task failed: {error}"),
        }
    }

    /// SUPER+CTRL+SPACE — the theme's next wallpaper.
    fn next_background(&mut self, cx: &mut Cx) {
        self.background_index += 1;
        let idx = self.background_index;
        self.apply_background(cx, idx);
    }

    fn apply_background(&mut self, cx: &mut Cx, index: usize) -> bool {
        // Wallpaper downloads and Super+Ctrl+Space update the Omarchy
        // raster layer only.
        if self.state_mut().style.target != desktop::DesktopStyle::Omarchy {
            return false;
        }
        let name = self.state_mut().theme_name.clone();
        let backgrounds = theme::theme_backgrounds(&name);
        let image = self.ui.widget(cx, ids!(bg_image));
        let image_ref = self.ui.image(cx, ids!(bg_image));
        let loaded = if !backgrounds.is_empty() {
            let path = &backgrounds[index % backgrounds.len()];
            image_ref.load_image_file_by_path_async(cx, path).is_ok()
        } else if name == theme::DEFAULT_THEME {
            // A stable cache key for embedded bytes; no filesystem lookup.
            image_ref.load_image_from_data_async(
                cx,
                std::path::Path::new("octosense-bundled/tokyo-night.webp"),
                std::sync::Arc::new(theme::BUNDLED_TOKYO_NIGHT_WALLPAPER),
            ).is_ok()
        } else {
            false
        };
        image.set_visible(cx, loaded);
        self.redraw_all(cx);
        loaded
    }

    fn open_shell_menu(&mut self, cx: &mut Cx, path: &str, skin: MenuSkin) {
        let path = if path.is_empty() && self.state_mut().style.target == desktop::DesktopStyle::NextStep {
            "workspace"
        } else { path };
        let menu = self.ui.widget(cx, ids!(shell_menu));
        {
            let mut borrowed = menu.borrow_mut::<ShellMenu>();
            if let Some(m) = borrowed.as_mut() {
                m.open_at(cx, path, skin);
            }
        }
        self.redraw_all(cx);
    }

    /// Rebuild the open menu's rows in place (a Settings row changed them).
    fn refresh_shell_menu(&mut self, cx: &mut Cx) {
        let menu = self.ui.widget(cx, ids!(shell_menu));
        if let Some(mut m) = menu.borrow_mut::<ShellMenu>() {
            m.refresh(cx);
        }
        self.redraw_all(cx);
    }

    fn close_shell_menu(&mut self, cx: &mut Cx) {
        let menu = self.ui.widget(cx, ids!(shell_menu));
        {
            let mut borrowed = menu.borrow_mut::<ShellMenu>();
            if let Some(m) = borrowed.as_mut() {
                m.close(cx);
            }
        }
        if let Some(focus) = self.state_mut().layout.focused_client() {
            self.focus_client(cx, focus);
        }
        self.redraw_all(cx);
    }

    /// The menu owns the keyboard while it is up (`Menu.qml` grabs it).
    fn shell_menu_key(&mut self, cx: &mut Cx, e: &KeyEvent) -> bool {
        let menu = self.ui.widget(cx, ids!(shell_menu));
        let mut consumed = false;
        {
            let mut borrowed = menu.borrow_mut::<ShellMenu>();
            if let Some(m) = borrowed.as_mut() {
                if m.is_open() {
                    consumed = m.key(cx, e);
                }
            }
        }
        if consumed {
            self.redraw_all(cx);
        }
        consumed
    }

    /// The menu owns the POINTER while it is up too, for the same reason
    /// `shell_menu_key` grabs the keyboard: `ShellMenu::handle_pointer`
    /// does its own raw-coordinate hit testing (card / row rects) instead
    /// of Makepad's `Event::hits` area-exclusivity, so it never marks a
    /// click "handled" — routed through the ordinary widget tree, the SAME
    /// click that activates a menu row also falls through the scrim to
    /// whatever tile is dimmed underneath it (a click on the launcher's
    /// Browser row also landed on — and clicked — the terminal tile behind
    /// it). Called directly, before `self.ui.handle_event` ever sees the
    /// event, so nothing else gets a look at it while the menu is open.
    fn shell_menu_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let menu = self.ui.widget(cx, ids!(shell_menu));
        let mut consumed = false;
        {
            let mut borrowed = menu.borrow_mut::<ShellMenu>();
            if let Some(m) = borrowed.as_mut() {
                if m.is_open() {
                    consumed = m.pointer(cx, event);
                }
            }
        }
        if consumed {
            self.redraw_all(cx);
        }
        consumed
    }

    /// As `shell_menu_pointer`, for a bar flyout: `ShellPanel::handle_event`
    /// has the identical raw-coordinate architecture (card / hit_at, no
    /// `Event::hits`), so a click, drag or release meant for an open panel
    /// (the clock/audio/power/monitor popup) would otherwise ALSO reach
    /// whatever tile sits behind the scrim. Scroll is left alone — the
    /// panel does nothing with it, so routing it here would only turn off
    /// SUPER+scroll workspace cycling for no reason.
    fn shell_panel_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        if matches!(event, Event::Scroll(_)) {
            return false;
        }
        let panel = self.ui.widget(cx, ids!(shell_panel));
        let is_open = panel
            .borrow::<shell::panels::ShellPanel>()
            .map(|p| p.open.is_some())
            .unwrap_or(false);
        if !is_open {
            return false;
        }
        panel.handle_event(cx, event, &mut Scope::empty());
        self.redraw_all(cx);
        true
    }

    /// The developer-mode banner owns presses on itself: Turn off ends the
    /// mode at once; the rest of the strip swallows the press.
    fn dev_banner_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let Event::MouseDown(e) = event else { return false };
        let (off, claims) = {
            let banner = self.ui.widget(cx, ids!(shell_dev_banner));
            let banner = banner.borrow::<shell::dev_banner::ShellDevBanner>();
            match banner.as_ref() {
                Some(b) => (b.off_hit(e.abs), b.claims(e.abs)),
                None => (false, false),
            }
        };
        if off {
            dev_mode::turn_off("banner");
            self.dev_mode_changed(cx);
        }
        claims
    }

    /// Developer mode moved (on, off, expired): re-announce every app's
    /// tools to the pane so its approvals follow, and redraw the banner.
    fn dev_mode_changed(&mut self, cx: &mut Cx) {
        let generation = dev_mode::generation();
        if generation == self.dev_generation {
            return;
        }
        self.dev_generation = generation;
        log!("wm: developer mode {}", if dev_mode::is_on() { "on" } else { "off" });
        for frame in self.ai_bus.reannounce() {
            self.send_to_pane(frame);
        }
        self.pane_links.reannounce();
        // App peers take `dev.run` and developer grants, or lose them.
        crate::host_tools::developer_mode_changed();
        for notice in dev_mode::take_notices() {
            self.notify(cx, "Developer mode", &notice);
        }
        self.redraw_all(cx);
    }

    /// Settings → Developer options (`setup.developer.*`, shell/menu.rs), and
    /// the phone's (About phone, revealed by seven taps on Build number:
    /// [`App::developer_build_tap`]). The one place outside dev_mode.rs that
    /// turns developer mode on: the person typed the confirmation phrase
    /// into the menu and chose the row, or confirmed Turn on in the phone's
    /// revealed Developer options. It turns on for the apps chosen under
    /// Apps it covers, which both show. A wider choice made while it is on
    /// waits for that confirmation again.
    fn developer_options_activate(&mut self, cx: &mut Cx, target: &str) {
        // Choosing apps keeps the menu open on the list.
        if let Some(choice) = target.strip_prefix(shell::menu::DEVELOPER_APPS_ROW).and_then(|t| t.strip_prefix('.')) {
            let scope = if choice == "all" {
                Some(dev_mode::Scope::AllApps)
            } else {
                let app = shell::menu::developer_app_of(choice);
                let every: Vec<String> = agents::all().into_iter().map(|a| a.id).collect();
                dev_mode::chosen_scope().toggled(&app, &every)
            };
            match scope {
                None => self.notify(cx, "Developer mode", "Choose at least one app, or All apps."),
                Some(scope) => match dev_mode::choose_apps(scope) {
                    Err(why) => self.notify(cx, "Developer mode", &why),
                    Ok(true) => self.notify(cx, "Developer mode", "Saved. It covers more apps once you turn it off and on again, with the confirmation."),
                    Ok(false) => {}
                },
            }
            self.refresh_shell_menu(cx);
            self.dev_mode_changed(cx);
            return;
        }
        self.close_shell_menu(cx);
        if let Some(typed) = target.strip_prefix("setup.developer.on:") {
            match dev_mode::PersonGesture::settings_phrase(typed) {
                None => self.notify(
                    cx,
                    "Developer mode is off",
                    &format!("To turn it on, type \u{201c}{}\u{201d} in Developer options, then choose Turn on.", dev_mode::CONFIRM_PHRASE),
                ),
                Some(gesture) => {
                    if let Err(why) = dev_mode::turn_on(gesture, dev_mode::chosen_scope()) {
                        self.notify(cx, "Developer mode is off", &why);
                    }
                }
            }
        } else if target == DEVELOPER_PHONE_ON {
            match &self.developer_options_revealed {
                Some(revealed) => {
                    if let Err(why) = dev_mode::turn_on(dev_mode::PersonGesture::phone_confirmed(revealed), dev_mode::chosen_scope()) {
                        self.notify(cx, "Developer mode is off", &why);
                    }
                }
                None => self.notify(cx, "Developer mode is off", "Tap Build number seven times first."),
            }
        } else if target == "setup.developer.off" {
            dev_mode::turn_off("settings");
        }
        self.dev_mode_changed(cx);
    }

    /// Whether About phone shows Developer options: revealed in this run, or
    /// developer mode is on.
    pub fn developer_options_shown(&self) -> bool {
        self.developer_options_revealed.is_some() || dev_mode::is_on()
    }

    /// Home's Settings › About phone › Developer options: toggle `app` in the
    /// apps developer mode covers (`None`: all apps). Only Settings' request
    /// handler calls it (`dev_mode` tests scan `phone/`).
    pub fn developer_choose(&mut self, cx: &mut Cx, app: Option<&str>) {
        if !self.developer_options_shown() {
            return;
        }
        let row = match app {
            Some(app) => shell::menu::developer_app_row(app),
            None => format!("{}.all", shell::menu::DEVELOPER_APPS_ROW),
        };
        self.developer_options_activate(cx, &row);
    }

    /// Home's Settings › About phone › Turn off developer mode.
    pub fn developer_turn_off(&mut self, cx: &mut Cx) {
        self.developer_options_activate(cx, "setup.developer.off");
    }

    /// Home's Settings › About phone › Developer options › Turn on, confirmed
    /// on its sheet with the apps it covers shown. Only after the gesture
    /// revealed Developer options; only Settings' request handler calls it.
    pub fn developer_phone_turn_on(&mut self, cx: &mut Cx) {
        self.developer_options_activate(cx, DEVELOPER_PHONE_ON);
    }

    /// One tap on Settings › About phone › Build number (Home's Settings,
    /// `phone/`): the phone's developer-options gesture, as on Android. The
    /// seventh reveals Developer options; it turns nothing on. What
    /// Settings shows the person. Only Settings' request handler calls it.
    pub fn developer_build_tap(&mut self, cx: &mut Cx) -> String {
        let _ = cx;
        if self.developer_options_shown() {
            return "Developer options are already shown.".into();
        }
        if !dev_mode::settings_available() {
            return "Developer mode needs a development build of OctoSense.".into();
        }
        match dev_mode::build_number_tap() {
            dev_mode::Tap::Remaining(left) if left > 4 => String::new(),
            dev_mode::Tap::Remaining(1) => "You are now 1 step away from showing developer options.".into(),
            dev_mode::Tap::Remaining(left) => format!("You are now {left} steps away from showing developer options."),
            dev_mode::Tap::Reached(reached) => {
                self.developer_options_revealed = Some(reached);
                "Developer options are shown below. Developer mode stays off until you turn it on there.".into()
            }
        }
    }

    /// What a menu row does. The ids are the jsonc's dotted paths, with
    /// `apps.<id>` and `style.theme[.import].<name>` from the providers.
    fn shell_menu_activate(&mut self, cx: &mut Cx, target: &str) {
        if target == shell::menu::ASK_APP_ROW {
            self.close_shell_menu(cx);
            self.ask_focused_app(cx);
            return;
        }
        if target == shell::menu::GLANCE_ROW {
            self.close_shell_menu(cx);
            self.set_glance_open(cx, true);
            return;
        }
        if target == shell::menu::SYSTEM_CHAT_ROW {
            self.close_shell_menu(cx);
            system_chat::open();
            self.focus_system_chat(cx);
            self.system_chat_changed(cx);
            return;
        }
        if target.starts_with(shell::menu::COMMANDS_ROW) || target == shell::menu::ASSISTANT_RESTART_ROW {
            self.assistant_commands_activate(cx, target);
            return;
        }
        if target == shell::menu::APPROVALS_ROW {
            self.close_shell_menu(cx);
            approvals::open_settings();
            self.approvals_changed(cx);
            return;
        }
        if target.starts_with("setup.developer.") {
            self.developer_options_activate(cx, target);
            return;
        }
        if target=="start.documents" {self.launch_app(cx,"files");return;}
        if target=="start.power" {self.toggle_shell_panel(cx,BarModule::Power);return;}
        #[cfg(not(mobile_only))]
        if let Some(name) = target.strip_prefix("desktop.") {
            if let Some(style) = desktop::DesktopStyle::parse(name) {
                if style.supports_dark() { self.state_mut().style.dark = name.ends_with("-dark"); }
                self.set_desktop_style(cx, style);
                return;
            }
        }
        if let Some(app) = target.strip_prefix("apps.") {
            let app = app.to_string();
            self.close_shell_menu(cx);
            self.launch_app(cx, &app);
            return;
        }
        if let Some(name) = target.strip_prefix("style.theme.import.") {
            let slug = name.replace(' ', "-");
            self.close_shell_menu(cx);
            match theme::import_omarchy_theme(&slug) {
                Ok(msg) => log!("wm: {}", msg),
                Err(err) => log!("wm: import failed: {}", err),
            }
            self.set_theme(cx, &slug);
            return;
        }
        if let Some(name) = target.strip_prefix("style.theme.") {
            let name = name.replace(' ', "-");
            self.close_shell_menu(cx);
            self.set_theme(cx, &name);
            return;
        }
        match target {
            "system.quit" => {
                if self.ask_before_quit(cx) {
                    cx.quit();
                }
            }
            "style.background" => {
                self.close_shell_menu(cx);
                self.next_background(cx);
            }
            "style.bar" => {
                self.close_shell_menu(cx);
                self.do_action(cx, WmAction::ToggleBar);
            }
            other => {
                log!("wm: menu row '{}' does nothing in nested mode", other);
                self.close_shell_menu(cx);
            }
        }
    }

    /// True where the shell bar has a clickable module under the point:
    /// those points answer Client to the drag query so the press reaches
    /// the widget instead of moving the OS window.
    fn shell_bar_claims(&self, cx: &mut Cx, p: Vec2d) -> bool {
        if self.state.as_ref().is_some_and(|state| state.style.target.mobile()) {
            return false;
        }
        let bar = self.ui.widget(cx, ids!(shell_bar));
        let borrowed = bar.borrow::<shell::bar::ShellBar>();
        borrowed
            .as_ref()
            .map(|b| b.module_at(p).is_some())
            .unwrap_or(false)
    }

    /// Show or hide the desktop's glance panel (glance_panel.rs), clear of the bar.
    fn set_glance_open(&mut self, cx: &mut Cx, open: bool) {
        // The overlay starts under the bar already: no clearance to add.
        if let Some(mut panel) = self.ui.widget(cx, ids!(shell_glance)).borrow_mut::<glance_panel::ShellGlancePanel>() {
            panel.set_open(open);
        }
        // Seen: the bar's glance button goes back from lit.
        if open {
            self.glance_seen_ms = glance::now_ms();
        }
        log!("wm: glance panel {}", if open { "open" } else { "closed" });
        if self.state.is_some() {
            self.update_bar(cx);
        }
        self.redraw_all(cx);
    }

    /// What the glance panel asked for: from a press (its widget action)
    /// or a key ([`glance_panel::ShellGlancePanel::key`]).
    fn glance_panel_action(&mut self, cx: &mut Cx, action: glance_panel::ShellGlancePanelAction) {
        match action {
            glance_panel::ShellGlancePanelAction::Open { app, route } => {
                log!("wm: glance card opens {} (route {:?})", app, route);
                self.set_glance_open(cx, false);
                self.launch_app(cx, &app);
            }
            // A card pressed in the panel: the card window, as its
            // notification opens it.
            glance_panel::ShellGlancePanelAction::OpenCard { key } => self.open_glance_card(cx, &key),
            glance_panel::ShellGlancePanelAction::Dismissed { count } => self.glance_dismissed(cx, count),
            glance_panel::ShellGlancePanelAction::Undo => self.glance_undo(cx),
            glance_panel::ShellGlancePanelAction::None => {}
        }
    }

    /// The person dismissed cards in the glance panel: a toast says so and
    /// offers Undo (one at a time: a newer dismissal replaces it, as it
    /// replaces what undo brings back).
    fn glance_dismissed(&mut self, cx: &mut Cx, count: usize) {
        if count == 0 {
            return;
        }
        let notes = self.ui.widget(cx, ids!(shell_notes));
        let Some(mut notes) = notes.borrow_mut::<shell::notifications::ShellNotifications>() else { return };
        if let Some(old) = self.glance_undo_toast.take() {
            notes.dismiss(cx, old);
        }
        let summary = if count == 1 { "Card dismissed".to_string() } else { format!("{count} cards cleared") };
        let id = notes.post(
            cx,
            shell::notifications::Notification {
                app: "wm".into(),
                summary,
                icon: Some(shell::ui::Ico::Check),
                action: Some("Undo".into()),
                urgency: shell::notifications::Urgency::Normal,
                ..Default::default()
            },
        );
        self.glance_undo_toast = Some(id);
    }

    /// Undo the last dismissal in the glance panel: its cards come back,
    /// and the keyboard's focus with them.
    fn glance_undo(&mut self, cx: &mut Cx) {
        let back = glance::undo_dismiss();
        log!("wm: glance undo brought back {} card(s)", back.len());
        if let Some(mut panel) = self.ui.widget(cx, ids!(shell_glance)).borrow_mut::<glance_panel::ShellGlancePanel>() {
            panel.focus_restored(&back);
        }
        if let Some(id) = self.glance_undo_toast.take() {
            if let Some(mut notes) = self.ui.widget(cx, ids!(shell_notes)).borrow_mut::<shell::notifications::ShellNotifications>() {
                notes.dismiss(cx, id);
            }
        }
        if self.state.is_some() {
            self.update_bar(cx);
        }
        self.redraw_all(cx);
    }

    /// A key for the glance panel while it holds the keyboard (glance_panel.rs
    /// `key`): true when it was the panel's.
    fn glance_key(&mut self, cx: &mut Cx, e: &KeyEvent) -> bool {
        let act = self.ui.widget(cx, ids!(shell_glance)).borrow_mut::<glance_panel::ShellGlancePanel>().and_then(|mut p| p.key(cx, e));
        let Some(act) = act else { return false };
        self.glance_panel_action(cx, act);
        if self.state.is_some() {
            self.update_bar(cx);
        }
        self.redraw_all(cx);
        true
    }

    fn glance_open(&mut self, cx: &mut Cx) -> bool {
        self.ui.widget(cx, ids!(shell_glance)).borrow::<glance_panel::ShellGlancePanel>().is_some_and(|p| p.open)
    }

    /// The card window (glance_sheet.rs) is modal: while a card is open
    /// every pointer event is its own (a press outside the card closes it).
    fn glance_sheet_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let sheet = self.ui.widget(cx, ids!(shell_glance_sheet));
        if !sheet.borrow::<glance_sheet::ShellGlanceSheet>().is_some_and(|s| s.is_open()) {
            return false;
        }
        sheet.handle_event(cx, event, &mut Scope::empty());
        if let Some(state) = self.state.as_mut() {
            state.phone.card_open = sheet.borrow::<glance_sheet::ShellGlanceSheet>().is_some_and(|s| s.is_open());
        }
        if matches!(event, Event::MouseDown(_) | Event::MouseUp(_) | Event::TouchUpdate(_)) {
            self.redraw_all(cx);
        }
        true
    }

    /// Close the top card before Back reaches the chat or app below it.
    fn close_glance_card(&mut self, cx: &mut Cx) -> bool {
        let sheet = self.ui.widget(cx, ids!(shell_glance_sheet));
        let closed = sheet.borrow_mut::<glance_sheet::ShellGlanceSheet>().is_some_and(|mut s| {
            if !s.is_open() { return false; }
            s.close(cx);
            true
        });
        if closed {
            if let Some(state) = self.state.as_mut() { state.phone.card_open = false; }
            self.redraw_all(cx);
        }
        closed
    }

    /// Open the published card `key` in the card window; when it is gone
    /// (withdrawn, expired), the glance panel instead.
    fn open_glance_card(&mut self, cx: &mut Cx, key: &str) {
        self.set_glance_open(cx, false);
        let phone = self.state.as_ref().is_some_and(|s| s.style.target.mobile());
        if phone {
            // Closing the card returns to its live glance feed. Chat turns
            // continue; only their panes are hidden, as with their Close button.
            self.close_chat_panes(cx, true);
            let state = self.state_mut();
            state.phone.shade.close();
            state.phone.navigate(mobile::PhoneScreen::Home);
            state.phone.pages.jump(-1);
            self.animate_phone(cx);
        }
        let insets = self.state.as_ref().map(|s| s.phone.insets).unwrap_or_default();
        let opened = self.ui.widget(cx, ids!(shell_glance_sheet)).borrow_mut::<glance_sheet::ShellGlanceSheet>().is_some_and(|mut s| {
            s.set_presentation(phone, insets);
            s.open_card(cx, key)
        });
        if phone { self.state_mut().phone.card_open = opened; }
        if opened {
            log!("wm: glance toast opens card {key}");
        } else {
            log!("wm: glance card {key} is gone; opening the glance feed");
            if !phone { self.set_glance_open(cx, true); }
        }
        self.redraw_all(cx);
    }

    /// As `shell_panel_pointer`, for the glance panel: while it is open a
    /// press anywhere (outside it, which closes it) and any pointer event
    /// over its column is its own, so its live cards get whole gestures.
    fn shell_glance_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let (p, press) = match event {
            Event::MouseDown(e) => (e.abs, true),
            Event::MouseMove(e) => (e.abs, false),
            Event::MouseUp(e) => (e.abs, false),
            Event::Scroll(e) => (e.abs, false),
            _ => return false,
        };
        let panel = self.ui.widget(cx, ids!(shell_glance));
        if !panel.borrow::<glance_panel::ShellGlancePanel>().is_some_and(|g| g.owns_pointer(p, press)) {
            return false;
        }
        panel.handle_event(cx, event, &mut Scope::empty());
        if press {
            self.redraw_all(cx);
        }
        true
    }

    /// Toggle a bar module's flyout, anchored to the module itself.
    fn toggle_shell_panel(&mut self, cx: &mut Cx, module: BarModule) {
        let Some(kind) = shell::panels::PanelKind::for_module(module) else {
            return;
        };
        let anchor = {
            let bar = self.ui.widget(cx, ids!(shell_bar));
            let borrowed = bar.borrow::<shell::bar::ShellBar>();
            borrowed
                .as_ref()
                .and_then(|b| b.module_rect(module))
                .unwrap_or_default()
        };
        let panel = self.ui.widget(cx, ids!(shell_panel));
        {
            let mut borrowed = panel.borrow_mut::<shell::panels::ShellPanel>();
            if let Some(p) = borrowed.as_mut() {
                p.toggle(cx, kind, anchor);
                self.shell_panel_open = p.open.map(|k| k.module());
            }
        }
        self.update_bar(cx);
    }

    /// Once a second: approval rules that ran out, `confirm: app` calls
    /// that waited too long, sheets nobody answered; then what the person
    /// must be told (every automatic approval among it).
    fn approvals_tick(&mut self, cx: &mut Cx) {
        approvals::tick();
        peer_link::tick();
        self.approvals_changed(cx);
        self.system_chat_changed(cx);
    }

    /// Agents the person turned off since the last tick: their live services
    /// go now (ADR 0004 §4), whichever way the app is hosted.
    fn revoke_agents(&mut self) {
        let revoked = approvals::take_revoked();
        // Agents just allowed get their peer; revoked ones are forgotten.
        agents::pump(&revoked);
        app_chat::pump();
        for app in revoked {
            let modules = self.module_host.revoke_assistant(&app);
            peer_link::revoke(&app);
            let contained = crate::ai_host::contained::revoke(&app);
            log!("approvals: {app}'s agent turned off; revoked {modules} module service(s){}", if contained { " and its contained peer" } else { "" });
        }
    }

    /// A phone's Back and Home reach the assistant panes, which draw over the
    /// home page full screen: Back closes the top one (an app's "Ask" panel,
    /// then the system chat), Home (`all`) closes both. Closing only hides a
    /// pane, as its own Close does: its conversation and a running turn stay.
    /// True when a pane was open.
    fn close_chat_panes(&mut self, cx: &mut Cx, all: bool) -> bool {
        let (app, system) = chat_panes_to_close(app_chat::is_open(), system_chat::is_open(), all);
        if app {
            app_chat::close();
        }
        if system {
            system_chat::close();
        }
        if app || system {
            self.system_chat_changed(cx);
        }
        app || system
    }

    /// The system chat moved (a frame from the kernel, the router decided
    /// one of its approvals): hand approvals on, and redraw.
    fn system_chat_changed(&mut self, cx: &mut Cx) {
        system_chat::pump();
        app_chat::pump();
        self.host_tools_pump(cx);
        let generation = system_chat::generation().wrapping_add(app_chat::generation());
        if generation != self.system_chat_generation {
            self.system_chat_generation = generation;
            self.redraw_all(cx);
        }
    }

    /// The host-tool relay (`host_tools`, octos UPCR-2026-035): calls,
    /// cancels, approvals and the router's decisions, on this thread; then
    /// the relay's own calls on the AI bus (the Terminal's `run`).
    fn host_tools_pump(&mut self, cx: &mut Cx) {
        host_tools::pump();
        let mut answered = false;
        for request in host_tools::take_bus_requests() {
            match request {
                host_tools::BusRequest::Call { call_id, app, tool, args } => match self.ai_bus.shell_call(&app, &tool, &args, &call_id) {
                    Some(route) => self.on_bus_route(cx, route),
                    None => {
                        let label = approvals::sheet::app_label(&app);
                        host_tools::bus_result(&call_id, ai_host::app_peers::host_tools::ToolOutcome::error("app_not_running", format!("Open {label} first: no running {label} offers {tool}")));
                        answered = true;
                    }
                },
                host_tools::BusRequest::Cancel { call_id } => {
                    if let Some(route) = self.ai_bus.shell_cancel(&call_id) {
                        self.on_bus_route(cx, route);
                    }
                }
            }
        }
        for call_id in self.ai_bus.take_failed_shell_calls() {
            host_tools::bus_result(&call_id, ai_host::app_peers::host_tools::ToolOutcome::error("app_exited", "the app closed before it answered"));
            answered = true;
        }
        if answered {
            host_tools::pump();
        }
    }

    /// "Ask <app>" for the focused window's app, when it has an agent
    /// (the bar's button, Shift+F8, Setup › Assistant).
    fn ask_focused_app(&mut self, cx: &mut Cx) {
        let app = self.focused_agent_app();
        match app {
            Some(app) => {
                log!("ask: {}'s agent", app.id);
                app_chat::open_app(app);
            }
            None => self.notify(cx, "No app agent here", "The focused app has no agent to ask. The system agent (F8) can help instead."),
        }
        self.system_chat_changed(cx);
    }

    /// The focused window's app, when it has an agent.
    fn focused_agent_app(&mut self) -> Option<apps::AgentApp> {
        let state = self.state.as_ref()?;
        let client = state.layout.focused_client()?;
        let app = state.clients.get(&client)?.app.clone();
        agents::find(&app)
    }

    /// "Ask <app>"'s pane owns the pointer inside its rect while open.
    fn app_chat_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        if !app_chat::is_open() {
            return false;
        }
        let pane = self.ui.widget(cx, ids!(shell_app_chat));
        let outcome = pane.borrow_mut::<system_chat::view::ShellSystemChat>().map(|mut p| p.pointer(cx, event)).unwrap_or(system_chat::view::Outcome::Ignored);
        match outcome {
            system_chat::view::Outcome::Ignored => false,
            _ => {
                self.system_chat_changed(cx);
                true
            }
        }
    }

    /// Text input, the input method's state query and its action key for
    /// the chat panes. True when a pane took the event. Only a pane whose
    /// prompt holds the key focus takes the input method's events; another
    /// focused field (an app's text input, a host sheet) keeps its own.
    fn chat_text_input(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let mut focused = [false; 2];
        for (i, pane) in [ids!(shell_app_chat), ids!(shell_system_chat)].into_iter().enumerate() {
            let pane = self.ui.widget(cx, pane);
            let Some(mut pane) = pane.borrow_mut::<system_chat::view::ShellSystemChat>() else { continue };
            focused[i] = pane.has_keyboard(cx);
            if !matches!(event, Event::ImeAction(_)) && pane.ime(cx, event) {
                return true;
            }
        }
        use system_chat::composer::{text_target, Pane};
        let target = match event {
            Event::ImeAction(action) if system_chat::composer::ime_action_sends(action.action) => system_chat::composer::ime_target(focused[0], focused[1]),
            // Typed text with no field focused (F8 opened the pane without
            // a press): the pane that has the keyboard. Never an input
            // method's whole editor state: that belongs to the field it was
            // asked of.
            Event::TextInput(t) if t.full_state_sync.is_none() => text_target(cx.key_focus().is_empty(), app_chat::is_focused(), system_chat::is_open()),
            _ => None,
        };
        match (event, target) {
            (Event::ImeAction(_), Some(Pane::App)) => app_chat::send_draft(),
            (Event::ImeAction(_), Some(Pane::System)) => system_chat::send_draft(),
            (Event::TextInput(t), Some(Pane::App)) => return app_chat::text_input(t),
            (Event::TextInput(t), Some(Pane::System)) => return system_chat::text_input(t),
            _ => return false,
        }
        true
    }

    /// Copy or cut (Command+C, Command+X) in the chat pane typing goes to:
    /// its prompt's selection, cut taking it out; else, for a copy, what is
    /// selected in its transcript. Without a selection the event is left to
    /// whoever else answers it.
    fn chat_clipboard(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let (response, cut) = match event {
            Event::TextCopy(e) => (e.response.clone(), false),
            Event::TextCut(e) => (e.response.clone(), true),
            _ => return false,
        };
        let panes = [ids!(shell_app_chat), ids!(shell_system_chat)];
        let mut focused = [false; 2];
        for (i, pane) in panes.into_iter().enumerate() {
            let pane = self.ui.widget(cx, pane);
            focused[i] = pane.borrow::<system_chat::view::ShellSystemChat>().is_some_and(|p| p.has_keyboard(cx));
        }
        use system_chat::composer::{text_target, Pane};
        let target = if focused[0] {
            Some(Pane::App)
        } else if focused[1] {
            Some(Pane::System)
        } else {
            text_target(cx.key_focus().is_empty(), app_chat::is_focused(), system_chat::is_open())
        };
        let selected = |cx: &mut Cx, pane| self.ui.widget(cx, pane).borrow::<system_chat::view::ShellSystemChat>().and_then(|p| p.selected_text());
        let text = match target {
            Some(Pane::App) => app_chat::copy_draft(cut).or_else(|| if cut { None } else { selected(cx, panes[0]) }),
            Some(Pane::System) => system_chat::copy_draft(cut).or_else(|| if cut { None } else { selected(cx, panes[1]) }),
            None => None,
        };
        match text {
            Some(text) => {
                *response.borrow_mut() = Some(text);
                true
            }
            None => false,
        }
    }

    /// The system chat just opened on a desktop: its prompt takes the
    /// keyboard, so an input method composes in it at once (a phone opens
    /// it with a tap, which gives the keyboard itself).
    fn focus_system_chat(&mut self, cx: &mut Cx) {
        if !system_chat::is_open() || self.state.is_none() || self.state_mut().style.target.mobile() {
            return;
        }
        if let Some(mut pane) = self.ui.widget(cx, ids!(shell_system_chat)).borrow_mut::<system_chat::view::ShellSystemChat>() {
            pane.focus_prompt(cx);
        }
    }

    /// A toast owns its native touch gesture through release, even after a
    /// drag cancels activation. Mouse handling takes only the press so a pane
    /// dragged across a toast keeps its own movement and release.
    fn press_on_toast(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let notes = self.ui.widget(cx, ids!(shell_notes));
        let on = notes.borrow_mut::<shell::notifications::ShellNotifications>()
            .is_some_and(|mut n| n.owns_pointer(event));
        on
    }

    /// The system chat's pane owns the pointer inside its rect while open.
    fn system_chat_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        if !system_chat::is_open() {
            return false;
        }
        let pane = self.ui.widget(cx, ids!(shell_system_chat));
        let outcome = pane.borrow_mut::<system_chat::view::ShellSystemChat>().map(|mut p| p.pointer(cx, event)).unwrap_or(system_chat::view::Outcome::Ignored);
        match outcome {
            system_chat::view::Outcome::Ignored => false,
            system_chat::view::Outcome::Taken => {
                self.system_chat_changed(cx);
                true
            }
            system_chat::view::Outcome::OpenProviders => {
                system_chat::close();
                self.launch_app(cx, "ai-providers");
                self.system_chat_changed(cx);
                true
            }
        }
    }

    /// Setup > Assistant > Command execution (`setup.assistant.commands.*`,
    /// shell/menu.rs). The one place outside system_chat/ that turns the
    /// system agent's command execution on: the person typed the
    /// confirmation into the menu and chose Allow.
    fn assistant_commands_activate(&mut self, cx: &mut Cx, target: &str) {
        self.close_shell_menu(cx);
        if let Some(typed) = target.strip_prefix(shell::menu::COMMANDS_ALLOW_ROW).and_then(|t| t.strip_prefix(':')) {
            match system_chat::grants::CommandGesture::settings_phrase(typed) {
                None => self.notify(
                    cx,
                    "The assistant may not run commands",
                    &format!("{} To allow it, type \u{201c}{}\u{201d} in Command execution, then choose Allow.", system_chat::grants::RISK, system_chat::grants::CONFIRM_PHRASE),
                ),
                Some(gesture) => match system_chat::grants::set_command_execution(true, Some(gesture)) {
                    Ok(()) => self.notify(cx, "Command execution is on", "Each command asks you on a sheet first. Restart the assistant to apply."),
                    Err(why) => self.notify(cx, "Command execution is off", &why),
                },
            }
        } else if target == shell::menu::COMMANDS_OFF_ROW {
            match system_chat::grants::set_command_execution(false, None) {
                Err(why) => self.notify(cx, "Command execution", &why),
                Ok(()) => self.notify(cx, "Command execution is off", "Restart the assistant to apply."),
            }
        } else if target == shell::menu::ASSISTANT_RESTART_ROW {
            if system_chat::grants::restart_assistant() {
                self.notify(cx, "Assistant restarting", "Its conversation resumes when it is back.");
            } else {
                self.notify(cx, "Assistant", "It isn't running; the change applies when it starts.");
            }
        }
        self.redraw_all(cx);
    }

    fn approvals_changed(&mut self, cx: &mut Cx) {
        self.revoke_agents();
        // The AI bus's held calls the router has answered go on (or are
        // refused to the pane).
        for (id, decision, reason) in approvals::take_bus_decisions() {
            let route = self.ai_bus.release(&id.0, decision.approved(), &reason);
            self.on_bus_route(cx, route);
        }
        for n in approvals::take_notices() {
            let shown = self.notify_shown(cx, &n.title, &n.body);
            if let Some(request) = n.request {
                self.approval_notices.record(request, shown);
            }
        }
        // "Needs you … Open the sheet" goes with its sheet line (answered,
        // stopped, withdrawn, expired): it pointed at a sheet that was gone.
        for (request, shown) in self.approval_notices.withdrawn(approvals::is_pending) {
            log!("approvals: {request} no longer waits; its notification is withdrawn");
            self.withdraw_notification(cx, shown);
        }
        let generation = approvals::generation();
        if generation != self.approvals_generation {
            self.approvals_generation = generation;
            self.redraw_all(cx);
        }
    }

    /// The in-process notification API — `WmRequest::Notify{title, body}`
    /// lands here.
    pub fn notify(&mut self, cx: &mut Cx, title: &str, body: &str) {
        self.notify_shown(cx, title, body);
    }

    /// [`Self::notify`], saying where it was shown (the desktop toast, the
    /// phone shade's notification), so it can be withdrawn.
    fn notify_shown(&mut self, cx: &mut Cx, title: &str, body: &str) -> approvals::Shown {
        let notes = self.ui.widget(cx, ids!(shell_notes));
        let toast = notes.borrow_mut::<shell::notifications::ShellNotifications>().map(|mut n| n.notify(cx, title, body));
        let now = cx.seconds_since_app_start();
        let shade = self.state.as_mut().map(|state| state.phone.shade.post("wm", title, body, now, Vec::new()));
        self.redraw_all(cx);
        approvals::Shown { toast, shade }
    }

    /// Take back what [`Self::notify_shown`] showed (gone already: nothing).
    fn withdraw_notification(&mut self, cx: &mut Cx, shown: approvals::Shown) {
        if let Some(id) = shown.toast {
            let notes = self.ui.widget(cx, ids!(shell_notes));
            let mut borrowed = notes.borrow_mut::<shell::notifications::ShellNotifications>();
            if let Some(n) = borrowed.as_mut() {
                n.dismiss(cx, id);
            }
        }
        if let (Some(id), Some(state)) = (shown.shade, self.state.as_mut()) {
            state.phone.shade.dismiss(id);
        }
        self.redraw_all(cx);
    }

    /// Announce a card that asked for it (`glance.publish` with `notify`):
    /// a toast on a desktop, which opens that card in the card window
    /// (glance_sheet.rs); a shade notification on the phone opens that same
    /// card, with its Glance feed underneath for Close/Back.
    fn glance_notify(&mut self, cx: &mut Cx, note: &glance::GlanceNote) {
        let body = if note.summary.is_empty() { "Open the card at a glance" } else { note.summary.as_str() };
        let notes = self.ui.widget(cx, ids!(shell_notes));
        // Who it is from (the app's own icon and name), what (the title,
        // without the app's name the publisher may have put first) and its
        // gist (the card's summary); a press opens the card.
        let name = app_display_name(&note.app);
        let title = note.title.strip_prefix(&format!("{name} \u{00b7} ")).unwrap_or(&note.title).to_string();
        let card_toast = shell::notifications::Notification {
            app: note.app.clone(),
            caption: name,
            summary: title,
            body: note.summary.clone(),
            icon: Some(shell::ui::Ico::Bell),
            app_icon: Some(note.open_app.clone()),
            urgency: shell::notifications::Urgency::Normal,
            // A card's toast stays its longest, so it can still be opened.
            requested: 30.0,
            ..Default::default()
        };
        if let Some(id) = notes.borrow_mut::<shell::notifications::ShellNotifications>().map(|mut n| n.post(cx, card_toast)) {
            self.glance_toasts.record(id, &note.key);
            log!("glance: toast {id} opens {}", note.key);
        }
        let now = cx.seconds_since_app_start();
        if let Some(state) = self.state.as_mut() {
            let id = state.phone.shade.post(&note.app, &note.title, body, now, Vec::new());
            self.glance_shade_notes.record(id, &note.key);
        }
        log!("glance: {} notifies {}", note.app, note.key);
        self.redraw_all(cx);
    }

    /// Show the volume OSD (the wheel over the audio module, and the
    /// volume keys).
    fn show_osd(&mut self, cx: &mut Cx, show: shell::osd::OsdShow) {
        let osd = self.ui.widget(cx, ids!(shell_osd));
        {
            let mut borrowed = osd.borrow_mut::<shell::osd::ShellOsd>();
            if let Some(o) = borrowed.as_mut() {
                o.present(cx, show);
            }
        }
        self.redraw_all(cx);
    }

    /// `osascript` the output volume, then say so on the OSD.
    fn set_volume(&mut self, cx: &mut Cx, level: u32) {
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("osascript")
                .args(["-e", &format!("set volume output volume {}", level)])
                .output();
        }
        self.bar_sample.volume = Some(level);
        self.show_osd(cx, shell::osd::OsdShow::volume(level, self.bar_sample.muted));
        self.update_bar(cx);
    }

    /// Feed the shell bar. Workspaces and the active window come from the
    /// WM; the status modules come from `bar_sample`, refreshed on the tick
    /// (`shell/bar.rs` does the sampling — cheap things every second, the
    /// expensive ones every fifth).
    fn update_bar(&mut self, cx: &mut Cx) {
        if let Some(clock)=self.bar_sample.clock.split_whitespace().find(|s|s.contains(':')).map(str::to_string) {
            self.state_mut().phone.clock=clock;
        }
        self.state_mut().phone.shade.battery = self.bar_sample.battery.map(|b| (b.percent, b.charging));
        self.wake_island(cx);
        let mut shown: Vec<usize> = Vec::new();
        let workspaces = {
            let state = self.state_mut();
            let active = state.layout.active;
            let mut cells: Vec<shell::bar::WorkspaceCell> = Vec::new();
            // Omarchy's bar: the active workspace is a dot, other POPULATED
            // ones show their number, empty ones are hidden.
            for i in 0..layout::WORKSPACES {
                let populated = !state.layout.clients_on(i).is_empty();
                if i != active && !populated {
                    continue;
                }
                shown.push(i);
                cells.push(shell::bar::WorkspaceCell {
                    label: format!("{}", (i + 1) % 10),
                    occupied: populated,
                    focused: i == active,
                });
            }
            cells
        };
        self.bar_workspaces = shown;
        let title = {
            let state = self.state_mut();
            state
                .layout
                .focused_client()
                .and_then(|c| state.clients.get(&c))
                .map(|s| s.display_title().to_string())
                .unwrap_or_default()
        };
        let mut data = self.bar_sample.clone();
        data.workspaces = workspaces;
        data.style = self.state_mut().style.target;
        data.dark = self.state_mut().style.dark;
        data.active_window = (!title.is_empty()).then_some(title);
        data.ask_agent = self.focused_agent_app().map(|a| a.name);
        data.open_panel = self.shell_panel_open;
        // The glance button: the cards the panel lists, and how many arrived
        // since it was last open; it wears the pill while the panel is up.
        let cards = glance::listed();
        let new = cards.iter().filter(|c| c.published_ms > self.glance_seen_ms).count();
        data.glance = Some((cards.len(), new));
        if self.glance_open(cx) {
            data.open_panel = Some(BarModule::Glance);
        }
        // The middle window control reads "restore" while maximized.
        data.maximized = self.ui.window(cx, ids!(main_window)).is_fullscreen(cx);
        let bar = self.ui.widget(cx, ids!(shell_bar));
        {
            let mut borrowed = bar.borrow_mut::<shell::bar::ShellBar>();
            if let Some(b) = borrowed.as_mut() {
                b.data = data;
                // Where the platform draws no caption buttons, the bar does.
                b.window_controls = shell::bar::window_controls_default();
            }
        }
        self.redraw_all(cx);
    }

    /// The bar's window controls: the same three calls the stock caption
    /// bar makes on its own buttons (widgets/src/window.rs).
    fn window_control(&mut self, cx: &mut Cx, module: BarModule) -> bool {
        let window = self.ui.window(cx, ids!(main_window));
        match module {
            BarModule::WindowMin => window.minimize(cx),
            BarModule::WindowMax => {
                if window.is_fullscreen(cx) {
                    window.restore(cx);
                } else {
                    window.maximize(cx);
                }
            }
            BarModule::WindowClose => window.close(cx),
            _ => return false,
        }
        log!("wm: window control {:?}", module);
        true
    }

    /// The bar IS this window's caption, so it has to be tall enough to
    /// center the OS window buttons and start its own content after them.
    /// Both numbers come from the platform's traffic-light rect (points,
    /// top-left origin) exactly like the stock caption bar does. Right-side
    /// caption buttons affect the height only; the bar's left cluster stays
    /// at its normal edge inset.
    fn update_bar_chrome(&mut self, cx: &mut Cx, geom: &WindowGeom) {
        // The standalone shell fills the window and keeps the insets for
        // itself: the wallpaper runs edge to edge, the status bar and the
        // navigation band sit inside the safe area (desk/phone.rs).
        #[cfg(mobile_only)]
        {
            let i = geom.safe_area_insets;
            let insets = mobile_gestures::SafeInsets { top: i.top, right: i.right, bottom: i.bottom, left: i.left };
            if self.state.is_some() && self.state_mut().phone.insets != insets {
                log!("wm: safe-area insets top {} right {} bottom {} left {}", i.top, i.right, i.bottom, i.left);
                self.state_mut().phone.insets = insets;
                self.redraw_all(cx);
            }
            return;
        }
        #[cfg(not(mobile_only))]
        self.update_desk_bar_chrome(cx, geom);
    }

    #[cfg(not(mobile_only))]
    fn update_desk_bar_chrome(&mut self, cx: &mut Cx, geom: &WindowGeom) {
        let native_mobile = cfg!(native_mobile);
        // Insets can change without changing the toolbar (rotation, system
        // navigation mode), so update them before the metrics cache check.
        if native_mobile {
            let insets = geom.safe_area_insets;
            let padding = Inset {
                top: insets.top, right: insets.right,
                bottom: insets.bottom, left: insets.left,
            };
            if let Some(mut body) = self.ui.view(cx, ids!(main_window.body)).borrow_mut() {
                body.layout.padding = padding;
                body.redraw(cx);
            }
        }
        let (height, pad_left) = bar_metrics_for_geom(geom, native_mobile);
        if self.bar_metrics == Some((height, pad_left)) {
            return;
        }
        self.bar_metrics = Some((height, pad_left));
        let mut bar = self.ui.widget(cx, ids!(bar));
        script_apply_eval!(cx, bar, {
            height: #(height)
        });
        // The bar's content starts after the OS window buttons.
        let shell_bar = self.ui.widget(cx, ids!(shell_bar));
        {
            let mut borrowed = shell_bar.borrow_mut::<shell::bar::ShellBar>();
            if let Some(b) = borrowed.as_mut() {
                b.pad_left = pad_left;
            }
        }
        if let Some(mut controls) = self.ui.widget(cx, ids!(phone_controls)).borrow_mut::<mobile_surface::PhoneSurface>() {
            controls.pad_left = pad_left;
        }
        self.redraw_all(cx);
    }

    /// The status modules. Volume every tick (it is one `osascript` and
    /// the user changes it constantly); battery, network and bluetooth
    /// every fifth, because they cost a process each.
    fn update_status(&mut self, cx: &mut Cx) {
        // The samplers fork subprocesses that can take hundreds of ms; on
        // this thread that starved the hosted tiles' 8ms Ticks (visible
        // ~0.5s hiccups in every child app). A background thread samples
        // and we only copy its cache here.
        if self.status_rx.is_none() {
            match shell::bar::start_status_sampler(&cx.thread_spawner()) {
                Ok((rx, worker)) => {
                    self.status_rx = Some(rx);
                    self.status_worker = Some(worker);
                }
                Err(error) => log!("wm: could not start status worker: {error}"),
            }
        }
        if let Some(rx) = &self.status_rx {
            while let Ok(sample) = rx.try_recv() {
                self.status_sample = sample;
            }
        }
        let s = self.status_sample.clone();
        self.bar_sample.volume = s.volume;
        self.bar_sample.muted = s.muted;
        // No sampler (no processes to fork `date` in: the web): the clock
        // is formatted from the platform's own epoch seconds instead.
        self.bar_sample.clock = match (self.clock_alt, s.clock.is_empty()) {
            (alt, true) => host::fallback_clock(alt),
            (true, false) => s.clock_alt,
            (false, false) => s.clock,
        };
        self.status_tick = self.status_tick.wrapping_add(1);
        self.bar_sample.battery = s.battery;
        self.bar_sample.network = s.network;
        self.bar_sample.bluetooth = s.bluetooth;
        // Keep the flyout's own copy in step with the bar's.
        let panel = self.ui.widget(cx, ids!(shell_panel));
        {
            let mut borrowed = panel.borrow_mut::<shell::panels::ShellPanel>();
            if let Some(p) = borrowed.as_mut() {
                p.data.volume = self.bar_sample.volume;
                p.data.muted = self.bar_sample.muted;
                p.data.battery = self.bar_sample.battery;
                p.data.network = self.bar_sample.network.map(|up| {
                    if up { "Connected".to_string() } else { "Not connected".to_string() }
                });
                p.data.bluetooth = self.bar_sample.bluetooth;
            }
        }
    }

    pub fn redraw_all(&mut self, cx: &mut Cx) {
        self.ui.redraw(cx);
    }

    // --------------------------------------------------------------
    // WM actions
    // --------------------------------------------------------------

    /// Hand the layout the true desk rect so fullscreen and the scratchpad
    /// console can reach past the outer gap.
    fn sync_geometry(&mut self, cx: &mut Cx) {
        let rect = self
            .desk(cx)
            .borrow_mut::<WmDesk>()
            .map(|d| d.desk_rect)
            .unwrap_or_default();
        if rect.size.x > 1.0 {
            self.state_mut().layout.set_outer(LRect::new(
                rect.pos.x,
                rect.pos.y,
                rect.size.x,
                rect.size.y,
            ));
        }
    }

    /// CTRL+ALT+DELETE — `omarchy-hyprland-window-close-all`: close every
    /// client the same polite way, one by one, then focus workspace 1.
    ///
    /// The pool's dormant instances go too. They are ordinary children and
    /// "close everything" must not leave four of them running; the pool
    /// notices they were asked to go (not that they crashed) and refills
    /// itself on the ticks that follow, so the desktop is empty AND the
    /// next window still opens instantly.
    ///
    /// `clients` holds the hidden warm viewers too, so a close-all takes
    /// the whole Quick-Look cache with it — hide keeps a viewer, this does
    /// not.
    fn close_all_windows(&mut self, cx: &mut Cx) {
        self.preview_cache.active = None;
        let all: Vec<ClientId> = self.state_mut().clients.keys().copied().collect();
        for client in all {
            self.request_close(cx, client);
        }
        self.preview_cache.clear();
        self.state_mut().layout.switch_workspace(0);
        self.redraw_all(cx);
    }

    /// The WM itself is going down: a hidden warm viewer has no tile and
    /// no window, so nothing else would ever reap it. Kill the cache's
    /// processes outright — `--stdin-loop` children do notice an orphaned
    /// parent eventually, but "eventually" is not a shutdown story.
    fn kill_warm_previews(&mut self, cx: &mut Cx) {
        let pool = cx.task_pool();
        for client in self.preview_cache.warm_clients() {
            if let Some(slot) = self.state_mut().clients.get_mut(&client) {
                if let Some(child) = slot.child.as_mut() {
                    log!("wm: shutdown kills warm preview client {}", client);
                    clients::kill_child_group(child, clients::GROUP_KILL_GRACE, &pool);
                }
            }
        }
        self.preview_cache.clear();
    }

    /// SUPER+F hides the bar with the window; anything that leaves
    /// fullscreen puts it back, unless SUPER+SHIFT+SPACE hid it.
    fn sync_bar_for_fullscreen(&mut self, cx: &mut Cx) {
        let fullscreen = {
            let layout = &self.state_mut().layout;
            let ws = layout.focus_ws();
            layout.workspaces[ws].fullscreen.is_some()
                && layout.workspaces[ws].fullscreen_mode == FullscreenMode::Fullscreen
        };
        let bar = self.ui.widget(cx, ids!(bar));
        if fullscreen && bar.visible() {
            bar.set_visible(cx, false);
            self.bar_hidden_by_fullscreen = true;
        } else if !fullscreen && self.bar_hidden_by_fullscreen {
            bar.set_visible(cx, true);
            self.bar_hidden_by_fullscreen = false;
        }
    }

    /// The layout picked a new focus on its own (a window closed, a client
    /// died): the bar follows it, and the keyboard goes to that tile —
    /// UNLESS the AI pane is open. The person typing in the chat asked for
    /// the close; an automatic refocus must not take their keys away
    /// mid-line (an `os.close` used to send the next console line into a
    /// sheet). A click on a tile still moves focus: that comes through
    /// `focus_client` directly, never through here.
    fn focus_after_layout(&mut self, cx: &mut Cx) {
        if self.ai_pane_is_open(cx) {
            self.update_bar(cx);
            self.focus_pane(cx);
            return;
        }
        if let Some(focus) = self.state_mut().layout.focused_client() {
            self.focus_client(cx, focus);
        }
    }

    fn do_action(&mut self, cx: &mut Cx, action: WmAction) {
        self.sync_geometry(cx);
        let area = self.desk_area(cx);
        let gap = self.state_mut().gap;
        let focus = self.state_mut().layout.focused_client();
        match action {
            WmAction::LaunchTerminal => self.launch_app(cx, "terminal"),
            WmAction::LaunchBrowser => self.launch_app(cx, "browser"),
            WmAction::CloseWindow => self.close_focused(cx),
            WmAction::CloseAllWindows => self.close_all_windows(cx),
            WmAction::ToggleSplit => {
                self.state_mut().layout.toggle_split();
            }
            WmAction::TogglePseudo => {
                if let Some(focus) = focus {
                    self.state_mut().layout.toggle_pseudo(focus, area, gap);
                }
            }
            WmAction::ToggleFloat => {
                if let Some(focus) = focus {
                    self.state_mut().layout.toggle_float(focus, area, gap);
                    self.focus_client(cx, focus);
                }
            }
            WmAction::PopOut => {
                if let Some(focus) = focus {
                    self.state_mut().layout.pop_out(focus, area, gap);
                    self.focus_client(cx, focus);
                }
            }
            WmAction::Fullscreen(mode) => {
                self.state_mut().layout.toggle_fullscreen_mode(mode);
                self.sync_bar_for_fullscreen(cx);
            }
            WmAction::TiledFullscreen => {
                if let Some(focus) = focus {
                    let on = self.state_mut().layout.toggle_client_fullscreen(focus);
                    // fullscreenstate 0 2: the client is told, the layout is
                    // untouched. Our children learn it as a custom message.
                    if let Some(slot) = self.state_mut().clients.get(&focus) {
                        if let Some(sender) = &slot.sender {
                            send_to_app(
                                sender,
                                vec![StudioToApp::Custom(format!(
                                    "{{\"wm_fullscreen\":{}}}",
                                    on
                                ))],
                            );
                        }
                    }
                }
            }
            WmAction::FocusDir(dir) => {
                if self.state_mut().layout.focus_dir(dir, area, gap) {
                    self.focus_after_layout(cx);
                }
            }
            WmAction::SwapDir(dir) => {
                self.state_mut().layout.swap_dir(dir, area, gap);
            }
            WmAction::ResizePx { axis, px } => {
                self.state_mut().layout.resize_px(axis, px, area, gap);
            }
            WmAction::CycleFocus(forward) => {
                self.state_mut().layout.cycle_focus(forward);
                self.focus_after_layout(cx);
            }
            WmAction::ToggleGroup => {
                self.state_mut().layout.toggle_group(area, gap);
                self.focus_after_layout(cx);
            }
            WmAction::MoveOutOfGroup => {
                self.state_mut().layout.move_out_of_group(area, gap);
                self.focus_after_layout(cx);
            }
            WmAction::MoveIntoGroup(dir) => {
                self.state_mut().layout.move_into_group(dir, area, gap);
                self.focus_after_layout(cx);
            }
            WmAction::GroupNext => {
                if self.state_mut().layout.group_cycle(true) {
                    self.focus_after_layout(cx);
                }
            }
            WmAction::GroupPrev => {
                if self.state_mut().layout.group_cycle(false) {
                    self.focus_after_layout(cx);
                }
            }
            WmAction::GroupActive(n) => {
                if self.state_mut().layout.group_set_active(n) {
                    self.focus_after_layout(cx);
                }
            }
            WmAction::Workspace(n) => {
                self.state_mut().layout.switch_workspace(n);
                self.focus_after_layout(cx);
                self.sync_bar_for_fullscreen(cx);
            }
            WmAction::MoveToWorkspace(n) => {
                self.state_mut().layout.move_focused_to_ex(n, true, area, gap);
                self.focus_after_layout(cx);
            }
            WmAction::MoveToWorkspaceSilent(n) => {
                self.state_mut()
                    .layout
                    .move_focused_to_ex(n, false, area, gap);
            }
            WmAction::WorkspaceNext => {
                // Hyprland `e+1`: cycle occupied workspaces, not a march
                // through the empty ones.
                let layout = &self.state_mut().layout;
                let n = layout.cycle_occupied(layout.active, true);
                self.state_mut().layout.switch_workspace(n);
                self.focus_after_layout(cx);
            }
            WmAction::WorkspacePrev => {
                let layout = &self.state_mut().layout;
                let n = layout.cycle_occupied(layout.active, false);
                self.state_mut().layout.switch_workspace(n);
                self.focus_after_layout(cx);
            }
            WmAction::WorkspaceFormer => {
                let former = self.state_mut().layout.former;
                self.state_mut().layout.switch_workspace(former);
                self.focus_after_layout(cx);
            }
            WmAction::ToggleScratchpad => {
                self.state_mut().layout.toggle_scratchpad();
                self.focus_after_layout(cx);
            }
            WmAction::MoveToScratchpad => {
                self.state_mut()
                    .layout
                    .move_focused_to_scratchpad(area, gap);
            }
            WmAction::ToggleWorkspaceLayout => {
                let mode = self.state_mut().layout.toggle_workspace_layout();
                // The scrolling algorithm is a follow-on lane; the flag is
                // live and the layout still draws dwindle.
                log!("wm: workspace layout -> {:?}", mode);
            }
            // The omarchy menu surface (shell/menu.rs): one card, the
            // jsonc tree, the `apps` provider as the launcher.
            WmAction::Menu => self.open_shell_menu(cx, "", MenuSkin::Menu),
            WmAction::AppsMenu => self.open_shell_menu(cx, "apps", MenuSkin::Launcher),
            WmAction::SystemMenu => self.open_shell_menu(cx, "system", MenuSkin::Menu),
            WmAction::MenuRoute(route) => self.open_shell_menu(cx, route, MenuSkin::Menu),
            WmAction::Keybindings => {
                self.open_shell_menu(cx, "learn.keybindings", MenuSkin::Menu)
            }
            WmAction::ThemeMenu => self.open_shell_menu(cx, "style.theme", MenuSkin::Menu),
            WmAction::BackgroundNext => self.next_background(cx),
            WmAction::ToggleBar => {
                let bar = self.ui.widget(cx, ids!(bar));
                let visible = bar.visible();
                bar.set_visible(cx, !visible);
                self.bar_hidden_by_fullscreen = false;
            }
            WmAction::ArmAltLayer => {
                self.alt_armed = true;
                log!("wm: SUPER+ALT layer armed for the next key");
                return;
            }
            WmAction::ToggleAi => self.toggle_ai_pane(cx),
            WmAction::ToggleGlance => {
                let open = self.ui.widget(cx, ids!(shell_glance)).borrow::<glance_panel::ShellGlancePanel>().is_some_and(|p| p.open);
                self.set_glance_open(cx, !open);
            }
        }
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    // --------------------------------------------------------------
    // SUPER + mouse (tiling.lua mouse:272 / mouse:273)
    // --------------------------------------------------------------

    fn begin_drag(&mut self, cx: &mut Cx, abs: Vec2d, resize: bool) -> bool {
        self.sync_geometry(cx);
        let area = self.desk_area(cx);
        let gap = self.state_mut().gap;
        let Some(client) = self
            .state_mut()
            .layout
            .client_at(abs.x, abs.y, area, gap)
        else {
            return false;
        };
        self.begin_drag_on(cx, client, abs, resize, false)
    }

    /// The drag itself, once the window is known. `armed` starts a drag
    /// that has ALREADY cleared the threshold — a tab torn off a groupbar
    /// is mid-gesture by the time it becomes a window drag.
    fn begin_drag_on(
        &mut self,
        cx: &mut Cx,
        client: ClientId,
        abs: Vec2d,
        resize: bool,
        armed: bool,
    ) -> bool {
        self.sync_geometry(cx);
        let area = self.desk_area(cx);
        let gap = self.state_mut().gap;
        let layout = &mut self.state_mut().layout;
        let floating = layout.is_float(client);
        let Some(rect) = layout.rect_of(client, area, gap) else {
            return false;
        };
        if floating {
            layout.raise_float(client);
        }
        let (cx_center, cy_center) = rect.center();
        self.drag = Some(DragState {
            client,
            resize,
            resize_x: true,
            resize_y: true,
            floating,
            start: abs,
            last: abs,
            start_rect: rect,
            grab_left: abs.x < cx_center,
            grab_top: abs.y < cy_center,
            armed,
            shift: false,
        });
        if floating {
            // Hyprland moves a drag 1:1 with the pointer, not on the
            // tile's 379ms layout tween (`WmDesk::draw_walk`'s sync loop
            // reads this to skip `retarget` for this one client).
            self.state_mut().dragging = vec![client];
        }
        self.focus_client(cx, client);
        true
    }

    fn drag_move(&mut self, cx: &mut Cx, abs: Vec2d, shift: bool) {
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        drag.shift = shift;
        // A press only becomes a drag once it clears the threshold.
        if !drag.armed {
            if (abs.x - drag.start.x).abs() < DRAG_THRESHOLD
                && (abs.y - drag.start.y).abs() < DRAG_THRESHOLD
            {
                return;
            }
            drag.armed = true;
            if !drag.resize {
                if let Some(w)=self.state.as_mut().and_then(|s|s.layout.desktop.get_mut(drag.client)).filter(|w|w.maximized || w.snap.is_some()) {
                    let anchor=((drag.start.x-drag.start_rect.x)/drag.start_rect.w).clamp(0.0,1.0);
                    w.rect.x=drag.start.x-w.rect.w*anchor;
                    w.rect.y=drag.start.y-16.0;
                    w.maximized=false;w.snap=None;drag.start_rect=w.rect;
                }
            }
        }
        let (client, resize, floating, start, last, start_rect, grab_left, grab_top) = (
            drag.client,
            drag.resize,
            drag.floating,
            drag.start,
            drag.last,
            drag.start_rect,
            drag.grab_left,
            drag.grab_top,
        );
        let (resize_x,resize_y)=(drag.resize_x,drag.resize_y);
        drag.last = abs;
        let area = self.desk_area(cx);
        let gap = self.state_mut().gap;
        if floating {
            let d = abs - start;
            let rect = if resize {
                // The grabbed corner moves with the pointer; the opposite
                // one is fixed (DragController.cpp:413-423).
                let (x, w) = if !resize_x {(start_rect.x,start_rect.w)}else if grab_left {
                    let w = (start_rect.w - d.x).max(80.0);
                    (start_rect.x + start_rect.w - w, w)
                } else {
                    (start_rect.x, (start_rect.w + d.x).max(80.0))
                };
                let (y, h) = if !resize_y {(start_rect.y,start_rect.h)}else if grab_top {
                    let h = (start_rect.h - d.y).max(60.0);
                    (start_rect.y + start_rect.h - h, h)
                } else {
                    (start_rect.y, (start_rect.h + d.y).max(60.0))
                };
                LRect::new(x, y, w, h)
            } else {
                LRect::new(start_rect.x + d.x, start_rect.y + d.y, start_rect.w, start_rect.h)
            };
            self.state_mut().layout.set_float_rect(client, rect);
        } else if resize {
            // Tiled: the divider follows the pointer, one frame at a time,
            // exactly like CDwindleAlgorithm::resizeTarget's Δ.
            let d = abs - last;
            // A tiled resize moves the split ratios: the grabbed edge
            // follows the pointer, so a left/top grab inverts the delta.
            let layout = &mut self.state_mut().layout;
            if d.x.abs() > 0.0 {
                layout.resize_px(Axis::Horizontal, d.x, area, gap);
            }
            if d.y.abs() > 0.0 {
                layout.resize_px(Axis::Vertical, d.y, area, gap);
            }
            let _ = (grab_left, grab_top);
        }
        // The drop hint: while SHIFT is down over some other tile, that
        // tile's ring turns accent — dropping there makes a tab, not a
        // swap. Only for a tiled move; a resize or a float has no such
        // drop.
        let (client, floating, resize) = {
            let d = self.drag.as_ref().unwrap();
            (d.client, d.floating, d.resize)
        };
        let hint = if shift && !floating && !resize {
            let area = self.desk_area(cx);
            let gap = self.state_mut().gap;
            self.state_mut()
                .layout
                .client_at(abs.x, abs.y, area, gap)
                .filter(|target| *target != client)
        } else {
            None
        };
        self.state_mut().drop_hint = hint;
        if floating && !resize && self.state_mut().style.target==desktop::DesktopStyle::Windows {
            self.state_mut().snap.drag(client,abs,area);
        }
        self.redraw_all(cx);
    }

    fn end_drag(&mut self, cx: &mut Cx, abs: Vec2d, shift: bool) {
        self.state_mut().dragging.clear();
        self.state_mut().drop_hint = None;
        let Some(drag) = self.drag.take() else {
            return;
        };
        // The button-up may not carry SHIFT any more (it is released
        // together with the mouse often enough); the drag's own last known
        // state stands in for it.
        let shift = shift || drag.shift;
        if drag.floating && !drag.resize && drag.armed && self.state_mut().style.target==desktop::DesktopStyle::Windows {
            let area=self.desk_area(cx);
            self.state_mut().snap.drag(drag.client,abs,area);
            if let Some(zone)=self.state_mut().snap.preview {
                // Preserve the free window's size before the edge drag.
                if let Some(w)=self.state_mut().layout.desktop.get_mut(drag.client) {w.rect=drag.start_rect;}
                self.apply_snap(cx,drag.client,zone);
            }
        }
        self.state_mut().snap.clear();
        if !drag.floating && !drag.resize && drag.armed {
            // Tiled move: dropping on another tile swaps the two, which is
            // what Hyprland's `movewindow` drag settles into.
            let area = self.desk_area(cx);
            let gap = self.state_mut().gap;
            if let Some(target) = self
                .state_mut()
                .layout
                .client_at(abs.x, abs.y, area, gap)
            {
                if target != drag.client {
                    let layout = &mut self.state_mut().layout;
                    if shift {
                        layout.group_drop(drag.client, target);
                    } else {
                        layout.swap_clients(drag.client, target);
                    }
                    self.focus_client(cx, drag.client);
                }
            }
        }
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    // --------------------------------------------------------------
    // Divider drag (a plain press IN THE GAP between two tiles)
    // --------------------------------------------------------------

    /// Grab the divider under `abs`, if the point is in a gap and not on
    /// any window. Returns false — leaving the press to the tiles — for
    /// every point that belongs to a client, so a drag that STARTS on a
    /// window behaves exactly as it always did.
    fn begin_divider_drag(&mut self, cx: &mut Cx, abs: Vec2d) -> bool {
        self.sync_geometry(cx);
        let area = self.desk_area(cx);
        let gap = self.state_mut().gap;
        // A window under the pointer keeps its own press. `client_at`
        // covers floats and the scratchpad too, so anything drawn OVER a
        // gap shadows the divider the way it shadows the wallpaper.
        if self
            .state_mut()
            .layout
            .client_at(abs.x, abs.y, area, gap)
            .is_some()
        {
            return false;
        }
        let Some(hit) = self.state_mut().layout.divider_at(abs.x, abs.y, area, gap) else {
            return false;
        };
        // Both sides of the split resize live: name every client under it
        // so `WmDesk` snaps their tiles instead of restarting the 379ms
        // layout tween on each pointer frame (see `WmState::dragging`).
        let clients = self.state_mut().layout.clients_under(&hit);
        log!(
            "wm: divider grab {:?} depth {} ratio {:.3} over {} tiles",
            hit.axis,
            hit.depth,
            hit.ratio,
            clients.len()
        );
        self.state_mut().dragging = clients;
        self.div_drag = Some(DividerDrag { hit, start: abs });
        self.apply_divider_cursor(cx, Some(hit.axis));
        true
    }

    fn divider_drag_move(&mut self, cx: &mut Cx, abs: Vec2d) {
        let Some(drag) = self.div_drag.as_ref() else {
            return;
        };
        let (hit, start) = (drag.hit, drag.start);
        let gap = self.state_mut().gap;
        let px = match hit.axis {
            Axis::Horizontal => abs.x - start.x,
            Axis::Vertical => abs.y - start.y,
        };
        // 1:1 with the pointer, always measured from the grab: no tween,
        // no accumulated delta, and a clamp at either end never eats part
        // of the way back.
        self.state_mut().layout.drag_divider_px(&hit, px, gap);
        self.apply_divider_cursor(cx, Some(hit.axis));
        self.redraw_all(cx);
    }

    fn end_divider_drag(&mut self, cx: &mut Cx) {
        self.state_mut().dragging.clear();
        if self.div_drag.take().is_none() {
            return;
        }
        log!("wm: divider drop");
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// Track the divider band under the pointer and wear the matching
    /// resize cursor over it. Called AFTER the tiles have seen the move:
    /// a tile's own hover-out resets the cursor to Default, and the last
    /// `set_cursor` of a frame is the one the platform applies.
    fn update_divider_cursor(&mut self, cx: &mut Cx, abs: Vec2d) {
        if self.state.is_none() || self.drag.is_some() || self.div_drag.is_some() {
            return;
        }
        let area = self.desk_area(cx);
        let gap = self.state_mut().gap;
        let on_client = self
            .state_mut()
            .layout
            .client_at(abs.x, abs.y, area, gap)
            .is_some();
        let axis = if on_client {
            None
        } else {
            self.state_mut()
                .layout
                .divider_at(abs.x, abs.y, area, gap)
                .map(|hit| hit.axis)
        };
        let was = self.div_hover;
        if axis != was {
            self.div_hover = axis;
            match axis {
                Some(a) => log!("wm: divider hover {:?}", a),
                None => log!("wm: divider hover none"),
            }
            // Leaving a band for bare desk: hand the cursor back. Leaving
            // it for a WINDOW does nothing — the tile's own hover-in just
            // set the cursor its child asked for.
            if axis.is_none() && was.is_some() && !on_client {
                self.apply_divider_cursor(cx, None);
            }
        }
        // Re-applied on every move while on a band, not only on the way
        // in: a tile's hover-out or a child's cursor request may have
        // reset it since the last one.
        if axis.is_some() {
            self.apply_divider_cursor(cx, axis);
        }
    }

    fn apply_divider_cursor(&mut self, cx: &mut Cx, axis: Option<Axis>) {
        cx.set_cursor(match axis {
            // A left|right split is moved sideways, a top/bottom one up
            // and down.
            Some(Axis::Horizontal) => MouseCursor::EwResize,
            Some(Axis::Vertical) => MouseCursor::NsResize,
            None => MouseCursor::Default,
        });
    }

    /// A groupbar tab dragged off its strip: the member leaves the group
    /// and takes a tile of its own, and the press carries on as the
    /// ordinary tiled SUPER-drag — drop it on another tile to swap, or
    /// with SHIFT to make it a tab there.
    fn tear_out_tab(&mut self, cx: &mut Cx, client: ClientId, abs: Vec2d) {
        let area = self.desk_area(cx);
        let gap = self.state_mut().gap;
        if !self.state_mut().layout.group_tear_out(client, area, gap) {
            return;
        }
        self.begin_drag_on(cx, client, abs, false, true);
        self.focus_client(cx, client);
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// SUPER + wheel: `focus({ workspace = "e+1" / "e-1" })`.
    fn scroll_workspace(&mut self, cx: &mut Cx, down: bool) {
        let layout = &self.state_mut().layout;
        let n = layout.cycle_occupied(layout.active, down);
        self.state_mut().layout.switch_workspace(n);
        self.focus_after_layout(cx);
        self.sync_bar_for_fullscreen(cx);
        self.update_bar(cx);
        self.redraw_all(cx);
    }

    /// One synthetic finger at `at`, entering the app through the same
    /// `handle_event` as a platform touch, so the phone's gestures, the
    /// desk and the modules see nothing unusual about it.
    fn synthetic_touch(&mut self, cx: &mut Cx, at: Vec2d, state: makepad_platform::event::TouchState) {
        use makepad_platform::event::{TouchPoint, TouchUpdateEvent};
        let time = cx.seconds_since_app_start();
        let event = Event::TouchUpdate(TouchUpdateEvent {
            time,
            window_id: CxWindowPool::id_zero(),
            modifiers: Default::default(),
            touches: vec![TouchPoint {
                state,
                abs: at,
                time,
                uid: 0x7e57,
                rotation_angle: 0.0,
                force: 0.0,
                radius: dvec2(1.0, 1.0),
                handled: Default::default(),
                sweep_lock: Default::default(),
            }],
        });
        self.handle_event(cx, &event);
    }

    /// The test actions' timers: a `capture:` tick writes the next frame; a
    /// due `ask-appcard:` sends its text to the appcard instance's executor
    /// exactly as the assistant's `ask` call would.
    fn fire_test_timers(&mut self, cx: &mut Cx, te: &TimerEvent) {
        if let Some((timer, path)) = self.test_capture.clone() {
            if timer.is_timer(te).is_some() {
                #[cfg(not(target_os = "android"))]
                {
                    let tmp = path.with_extension("part.png");
                    cx.capture_next_frame_to_file(tmp);
                    let _ = std::fs::rename(path.with_extension("part.png"), path);
                }
                #[cfg(target_os = "android")]
                {
                    for result in cx.try_take_texture_readbacks() {
                        if Some(result.ticket) != self.test_capture_ticket { continue; }
                        self.test_capture_ticket = None;
                        let output = if self.test_recording {
                            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
                            path.join(format!("{timestamp}.png"))
                        } else { path.clone() };
                        match cx.task_pool().submit(Lane::Heavy, move || {
                            let Ok(bytes) = result.data else { return; };
                            let mut rgba = Vec::with_capacity(result.width * result.height * 4);
                            for y in 0..result.height {
                                let row = match result.origin { ReadbackOrigin::TopLeft => y, ReadbackOrigin::BottomLeft => result.height - 1 - y };
                                for pixel in bytes[row*result.stride..row*result.stride+result.width*4].chunks_exact(4) {
                                    match result.channel_order {
                                        ReadbackChannelOrder::Rgba => rgba.extend_from_slice(pixel),
                                        ReadbackChannelOrder::Bgra => rgba.extend_from_slice(&[pixel[2],pixel[1],pixel[0],pixel[3]]),
                                    }
                                }
                            }
                            if let Ok(png) = Cx::encode_rgba_as_png(result.width as u32, result.height as u32, &rgba) {
                                let tmp = output.with_extension("part.png");
                                if std::fs::write(&tmp,png).is_ok() { let _ = std::fs::rename(tmp,output); }
                            }
                        }) { Ok(task) => task.detach(), Err(error) => log!("wm: capture worker unavailable: {error}") }
                    }
                    if self.test_capture_ticket.is_none() {
                        let focus = self.state_mut().layout.focused_client();
                        let texture = focus.and_then(|client|self.desk(cx).borrow::<WmDesk>().and_then(|desk|desk.phone_client_texture(client)));
                        if let Some(texture) = texture {
                            self.test_capture_ticket = texture.read_back(cx, ReadbackRequest::default()).ok();
                            cx.redraw_all();
                        }
                    }
                }
            }
        }
        if let Some((timer, n)) = &self.test_page {
            if timer.is_timer(te).is_some() {
                let n = *n;
                self.test_page = None;
                self.state_mut().phone.pages.jump(n);
                self.animate_phone(cx);
            }
        }
        if let Some(pos) = self.test_taps.iter().position(|(t, _, _)| t.is_timer(te).is_some()) {
            use makepad_platform::event::TouchState;
            let (_, at, state) = self.test_taps.remove(pos);
            self.synthetic_touch(cx, at, state);
            // The finger lifts a beat later, as a real tap's does; a
            // hold is measured from the touch times.
            if state == TouchState::Start {
                let timer = cx.start_timeout(0.08);
                self.test_taps.push((timer, at, TouchState::Stop));
            }
        }
        let Some(pos) = self.test_asks.iter().position(|(t, _)| t.is_timer(te).is_some()) else {
            return;
        };
        let (_, text) = self.test_asks.remove(pos);
        let Some(client) = self.module_host.client_of_module("appcard") else {
            log!("wm: ask-appcard {:?}: no appcard instance is running", text);
            return;
        };
        let call = ServiceCall {
            call_id: format!("test-ask-{}", self.next_id),
            tool: "ask".into(),
            args: format!("{{\"text\":{}}}", text.serialize_json()),
        };
        let outcome = self.module_host.execute(cx, client, &call);
        log!("wm: ask-appcard {:?} -> client {} outcome {:?}", text, client, outcome.map(|o| matches!(o, ExecOutcome::Done(_))));
        self.redraw_all(cx);
    }

    /// `--test-action <name>` fires one WmAction at startup, so every
    /// binding can be driven from a script even where the host OS keeps a
    /// chord for itself.
    fn run_test_actions(&mut self, cx: &mut Cx) {
        let mut args: Vec<String> = std::env::args().collect();
        if let Ok(config) = std::env::var("MAKEPAD_APP_CONFIG") {
            if let Ok(config) = makepad_strict_json::parse(config.as_bytes()) {
                if let Some(actions) = config.get("test_actions").and_then(|v| v.as_arr()) {
                    for action in actions.iter().filter_map(|v| v.as_str()) {
                        args.extend(["--test-action".into(), action.into()]);
                    }
                }
            }
        }
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--test-action" {
                if let Some(name) = args.get(i + 1) {
                    // launch-<app id>: spawn a registered app directly — the
                    // deterministic way to put one app on the desk in a test.
                    if self.groups_test_action(cx, name) { i += 2; continue; }
                    // webview-crawl:<url>,<url>…: a small crawl through the
                    // octos reader with the hidden WebView renderer, one
                    // `[webview-crawl]` log line per page (on-device check).
                    // toolbox-research:<topic>: a full toolbox research run
                    // (topic-brief, then deep_crawl) as an app agent's call
                    // would make it; `[toolbox-research]` log lines.
                    #[cfg(feature = "toolbox-peers")]
                    if let Some(topic) = name.strip_prefix("toolbox-research:") {
                        log!("wm: --test-action toolbox-research {}", topic);
                        crate::host_tools::toolbox::research_test(topic);
                        i += 2;
                        continue;
                    }
                    #[cfg(feature = "toolbox-peers")]
                    if let Some(seeds) = name.strip_prefix("webview-crawl:") {
                        let seeds: Vec<String> = seeds
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| s.starts_with("http"))
                            .collect();
                        log!("wm: --test-action webview-crawl {} seeds", seeds.len());
                        octosense_ai_host::webview_render::crawl_test(seeds);
                        i += 2;
                        continue;
                    }
                    if let Some(app) = name.strip_prefix("launch-") {
                        let app = app.to_string();
                        log!("wm: --test-action launch {}", app);
                        self.launch_app(cx, &app);
                        i += 2;
                        continue;
                    }
                    // capture:<path>: write the presented frame to <path>
                    // every 5 s — a scripted run's screen, with no display
                    // (or a locked one) needed.
                    if let Some(path) = name.strip_prefix("capture:") {
                        log!("wm: --test-action capture -> {}", path);
                        let timer = cx.start_interval(5.0);
                        self.test_capture = Some((timer, std::path::PathBuf::from(path)));
                        i += 2;
                        continue;
                    }
                    #[cfg(target_os = "android")]
                    if let Some(path) = name.strip_prefix("record:") {
                        let path = std::path::PathBuf::from(path);
                        if std::fs::create_dir_all(&path).is_ok() {
                            self.test_recording = true;
                            self.test_capture = Some((cx.start_interval(0.1), path));
                        }
                        i += 2;
                        continue;
                    }
                    // page:<n>: jump the phone home to page <n> (-1 is the
                    // glance page, the last position the App Library), on
                    // the iOS shell when the desktop is not a phone yet.
                    if let Some(n) = name.strip_prefix("page:") {
                        let n = n.trim().parse::<i64>().unwrap_or(0);
                        log!("wm: --test-action page {}", n);
                        if !self.state_mut().style.target.mobile() { self.set_desktop_style(cx, desktop::DesktopStyle::Ios); }
                        // After the first frames: the home's pages are laid
                        // out at the phone's size by then.
                        let timer = cx.start_timeout(1.0);
                        self.test_page = Some((timer, n));
                        i += 2;
                        continue;
                    }
                    // taps:<x>,<y>@<s>[;…]: a finger down and up at window
                    // point (x, y) <s> seconds after startup, for a run with
                    // no touch input — the headless iOS simulator first of all.
                    if let Some(spec) = name.strip_prefix("taps:") {
                        match parse_test_taps(spec) {
                            Ok(taps) => {
                                for (delay, at) in taps {
                                    log!("wm: --test-action tap {:?} in {}s", at, delay);
                                    let timer = cx.start_timeout(delay);
                                    self.test_taps.push((timer, at, makepad_platform::event::TouchState::Start));
                                }
                            }
                            Err(error) => log!("wm: --test-action taps: {}", error),
                        }
                        i += 2;
                        continue;
                    }
                    // ask-appcard:<text>: submit <text> to the hosted AppCard's
                    // composer (its `ask` tool over the bus) after a delay —
                    // `OCTOSENSE_TEST_ASK_DELAY` seconds, default 25 — so the
                    // app's kernel and sessions are up when the text lands.
                    if let Some(text) = name.strip_prefix("ask-appcard:") {
                        let delay = std::env::var("OCTOSENSE_TEST_ASK_DELAY")
                            .ok()
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or(25.0);
                        log!("wm: --test-action ask-appcard {:?} in {}s", text, delay);
                        let timer = cx.start_timeout(delay);
                        self.test_asks.push((timer, text.to_string()));
                        i += 2;
                        continue;
                    }
                    // style:<android|ios>: switch to that phone shell first,
                    // so a later launch-<app id> opens the app at a phone's
                    // size (an app's phone layout, checked on a desktop).
                    if let Some(style) = name.strip_prefix("style:") {
                        match style.trim() {
                            "android" => self.set_desktop_style(cx, desktop::DesktopStyle::Android),
                            "ios" => self.set_desktop_style(cx, desktop::DesktopStyle::Ios),
                            other => log!("wm: --test-action style: no phone style {other:?} (android or ios)"),
                        }
                        i += 2;
                        continue;
                    }
                    // shade:<notifications|controls>: open the shade on that
                    // side (switching to the Android phone shell first when
                    // the desk is up), for scripted screenshots.
                    if let Some(side) = name.strip_prefix("shade:") {
                        let side = if side.trim() == "controls" { mobile_gestures::ShadeSide::Controls } else { mobile_gestures::ShadeSide::Notifications };
                        log!("wm: --test-action shade -> {:?}", side);
                        if !self.state_mut().style.target.mobile() { self.set_desktop_style(cx, desktop::DesktopStyle::Android); }
                        self.state_mut().phone.shade.open_on(side);
                        self.animate_phone(cx);
                        i += 2;
                        continue;
                    }
                    if self.island_test_action(cx, name) { i += 2; continue; }
                    // perf:on / perf:off: the phone shell's frame-time
                    // reporter (mobile_perf.rs), a `[perf]` line every 2 s.
                    if let Some(what) = name.strip_prefix("perf:") {
                        mobile_perf::set_enabled(cx, what.trim() != "off");
                        self.animate_phone(cx);
                        i += 2;
                        continue;
                    }
                    // approval-sheet, approval-batch, approval-consent,
                    // approvals-settings (approvals/mod.rs).
                    // system-chat, system-chat-send:<text> (system_chat/).
                    // ask:<app>, ask-send:<text> (app_chat/).
                    if system_chat::test_action(name) || app_chat::test_action(name) {
                        log!("wm: --test-action {}", name);
                        self.system_chat_changed(cx);
                        i += 2;
                        continue;
                    }
                    if approvals::test_action(name) {
                        log!("wm: --test-action {}", name);
                        self.redraw_all(cx);
                        i += 2;
                        continue;
                    }
                    match test_action(name) {
                        Some(action) => {
                            log!("wm: --test-action {} -> {:?}", name, action);
                            self.do_action(cx, action);
                        }
                        None => log!("wm: unknown --test-action '{}'", name),
                    }
                }
                i += 2;
            } else {
                i += 1;
            }
        }
    }
}

/// `--test-action taps:` entries: `<x>,<y>@<seconds>` separated by `;`,
/// blanks around every number tolerated. An entry that does not read so
/// fails the whole list, named, rather than tapping somewhere else.
fn parse_test_taps(spec: &str) -> Result<Vec<(f64, Vec2d)>, String> {
    spec.split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let parsed = entry.split_once('@').and_then(|(point, delay)| {
                let (x, y) = point.split_once(',')?;
                Some((
                    delay.trim().parse::<f64>().ok()?,
                    dvec2(x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?),
                ))
            });
            parsed.ok_or_else(|| format!("tap {entry:?} is not <x>,<y>@<seconds>"))
        })
        .collect()
}

/// Names accepted by `--test-action`. Anything the keymap binds is
/// reachable by its Omarchy description, lowercased with dashes.
fn test_action(name: &str) -> Option<WmAction> {
    let name = name.trim().to_lowercase();
    let simple = match name.as_str() {
        "terminal" => Some(WmAction::LaunchTerminal),
        "close" => Some(WmAction::CloseWindow),
        "close-all" => Some(WmAction::CloseAllWindows),
        "toggle-split" => Some(WmAction::ToggleSplit),
        "pseudo" => Some(WmAction::TogglePseudo),
        "float" => Some(WmAction::ToggleFloat),
        "pop" => Some(WmAction::PopOut),
        "fullscreen" => Some(WmAction::Fullscreen(FullscreenMode::Fullscreen)),
        "maximize" => Some(WmAction::Fullscreen(FullscreenMode::Maximized)),
        "tiled-fullscreen" => Some(WmAction::TiledFullscreen),
        "focus-left" => Some(WmAction::FocusDir(Dir::Left)),
        "focus-right" => Some(WmAction::FocusDir(Dir::Right)),
        "focus-up" => Some(WmAction::FocusDir(Dir::Up)),
        "focus-down" => Some(WmAction::FocusDir(Dir::Down)),
        "swap-left" => Some(WmAction::SwapDir(Dir::Left)),
        "swap-right" => Some(WmAction::SwapDir(Dir::Right)),
        "swap-up" => Some(WmAction::SwapDir(Dir::Up)),
        "swap-down" => Some(WmAction::SwapDir(Dir::Down)),
        "grow" => Some(WmAction::ResizePx {
            axis: Axis::Horizontal,
            px: 100.0,
        }),
        "shrink" => Some(WmAction::ResizePx {
            axis: Axis::Horizontal,
            px: -100.0,
        }),
        "group" => Some(WmAction::ToggleGroup),
        "group-out" => Some(WmAction::MoveOutOfGroup),
        "group-into-left" => Some(WmAction::MoveIntoGroup(Dir::Left)),
        "group-into-right" => Some(WmAction::MoveIntoGroup(Dir::Right)),
        "group-next" => Some(WmAction::GroupNext),
        "group-prev" => Some(WmAction::GroupPrev),
        "scratchpad" => Some(WmAction::ToggleScratchpad),
        "to-scratchpad" => Some(WmAction::MoveToScratchpad),
        "workspace-next" => Some(WmAction::WorkspaceNext),
        "workspace-prev" => Some(WmAction::WorkspacePrev),
        "workspace-former" => Some(WmAction::WorkspaceFormer),
        "cycle" => Some(WmAction::CycleFocus(true)),
        "cycle-back" => Some(WmAction::CycleFocus(false)),
        "menu" => Some(WmAction::Menu),
        "apps" => Some(WmAction::AppsMenu),
        "system" => Some(WmAction::SystemMenu),
        "keys" => Some(WmAction::Keybindings),
        "theme" => Some(WmAction::ThemeMenu),
        "background" => Some(WmAction::BackgroundNext),
        "ai" => Some(WmAction::ToggleAi),
        "glance" => Some(WmAction::ToggleGlance),
        "bar" => Some(WmAction::ToggleBar),
        "layout" => Some(WmAction::ToggleWorkspaceLayout),
        _ => None,
    };
    if simple.is_some() {
        return simple;
    }
    if let Some(n) = name.strip_prefix("workspace-") {
        return n.parse::<usize>().ok().map(|n| WmAction::Workspace(n - 1));
    }
    if let Some(n) = name.strip_prefix("move-to-") {
        return n
            .parse::<usize>()
            .ok()
            .map(|n| WmAction::MoveToWorkspace(n - 1));
    }
    if let Some(n) = name.strip_prefix("group-") {
        return n.parse::<usize>().ok().map(WmAction::GroupActive);
    }
    None
}

/// The SUPER chord for mouse binds — Ctrl+Alt nested, the Logo key on a
/// Linux session (binds.rs carries the same law for keys).
/// No modifier at all: the bare function keys the WM owns (F10).
/// Whether an admitted hub socket may bind `slot` (ADR 0004 §5): only a
/// live process client's slot that no socket holds yet. An in-process
/// module has no process, so it is never bound to a socket; a bound slot is
/// never rebound, not even after its socket drops.
fn slot_accepts_socket(slot: Option<&clients::ClientSlot>) -> bool {
    slot.is_some_and(|slot| slot.child.is_some() && !slot.stopped && slot.socket.is_none() && slot.sender.is_none())
}

/// Whether frames from hub socket `socket` speak for `slot`: only while it
/// is the socket bound to that live slot.
fn frame_is_bound(slot: Option<&clients::ClientSlot>, socket: u64) -> bool {
    slot.is_some_and(|slot| slot.socket == Some(socket))
}

/// An app's name for people: a system app's manifest name, else a label
/// made from its id.
fn app_display_name(app: &str) -> String {
    #[cfg(any(feature = "app-hub", native_mobile))]
    return crate::glance_notice::app_name(app);
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    crate::approvals::sheet::app_label(app)
}

fn bare_key(m: &KeyModifiers) -> bool {
    !m.shift && !m.control && !m.alt && !m.logo
}

/// The `os.launch` answer: an app that was already running is in front
/// now and its tools are ready this turn; one that is starting connects
/// in a moment (the engine holds this result until it does).
fn os_launch_answer(call_id: &str, label: &str, already_running: bool) -> ToolResult {
    if already_running {
        ToolResult::ok(call_id, format!("{label} is already running and now in front; its tools are available now"), "already running")
    } else {
        ToolResult::ok(call_id, format!("{label} is starting; its tools become available once it connects"), "starting")
    }
}

/// The registry's ids, for a refusal that names what exists.
fn known_app_ids() -> String {
    clients::available_apps()
        .iter()
        .filter(|a| apps::listed(&a.id))
        .map(|a| a.id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// One row of `os.list`: what the model reads as text and as data.
#[derive(Clone, Debug, PartialEq, SerJson)]
struct OsAppRow {
    id: String,
    label: String,
    running: bool,
    focused: bool,
}

/// `os.list`'s answer: one line per app, and the same rows as JSON.
fn os_list_result(call_id: &str, rows: &[OsAppRow]) -> ToolResult {
    let text = rows
        .iter()
        .map(|r| {
            let state = if r.focused {
                "focused"
            } else if r.running {
                "running"
            } else {
                "not running"
            };
            format!("{} (`{}`) — {}", r.label, r.id, state)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let running = rows.iter().filter(|r| r.running).count();
    ToolResult::ok(
        call_id,
        text,
        format!("{} apps, {} running", rows.len(), running),
    )
    .with_data(rows.to_vec().serialize_json())
}

#[cfg(all(test, unix))]
mod hub_binding_tests {
    use super::*;

    fn process_slot(id: ClientId) -> clients::ClientSlot {
        let mut slot = clients::ClientSlot::module(id, "terminal", "Terminal");
        slot.child = Some(clients::ProcessGroup::new(std::process::Command::new("true").spawn().expect("spawn true")));
        slot
    }

    #[test]
    fn should_bind_a_socket_only_when_the_slot_is_an_unbound_live_process() {
        let mut slot = process_slot(4);
        assert!(slot_accepts_socket(Some(&slot)));
        assert!(!slot_accepts_socket(None), "no slot: nothing to bind");
        assert!(!slot_accepts_socket(Some(&clients::ClientSlot::module(5, "reference", "Reference"))), "a module slot never takes a socket");
        // Bound: a second socket is refused, and so is a rebind after the
        // first socket dropped.
        let (tx, _rx) = std::sync::mpsc::channel();
        slot.socket = Some(11);
        slot.sender = Some(tx);
        assert!(!slot_accepts_socket(Some(&slot)));
        slot.stopped = true;
        slot.socket = None;
        slot.sender = None;
        assert!(!slot_accepts_socket(Some(&slot)), "a stopped slot is not live");
        let _ = slot.child.take().map(|mut c| c.wait());
    }

    #[test]
    fn should_ignore_frames_when_the_socket_is_not_bound_to_the_slot() {
        let mut slot = process_slot(4);
        assert!(!frame_is_bound(Some(&slot), 11), "not bound yet");
        slot.socket = Some(11);
        assert!(frame_is_bound(Some(&slot), 11));
        assert!(!frame_is_bound(Some(&slot), 12), "another socket naming the same id");
        assert!(!frame_is_bound(None, 11), "a slot that is gone");
        let _ = slot.child.take().map(|mut c| c.wait());
    }
}

/// Which assistant panes a phone's Back closes (`all` false: the top one, an
/// app's "Ask" panel over the system chat) or its Home closes (`all`: both),
/// given which are open: (the app's panel, the system chat).
fn chat_panes_to_close(app_open: bool, system_open: bool, all: bool) -> (bool, bool) {
    (app_open, system_open && (all || !app_open))
}

#[cfg(test)]
mod chat_pane_back_tests {
    use super::chat_panes_to_close;

    #[test]
    fn back_closes_the_top_pane_and_home_closes_both() {
        // Back: the top pane only; nothing open leaves Back to the app.
        assert_eq!(chat_panes_to_close(false, true, false), (false, true), "the system chat alone");
        assert_eq!(chat_panes_to_close(true, true, false), (true, false), "the app's panel is on top");
        assert_eq!(chat_panes_to_close(true, false, false), (true, false));
        assert_eq!(chat_panes_to_close(false, false, false), (false, false), "no pane: Back goes to the app");
        // Home: every open pane.
        assert_eq!(chat_panes_to_close(true, true, true), (true, true));
        assert_eq!(chat_panes_to_close(false, true, true), (false, true));
        assert_eq!(chat_panes_to_close(false, false, true), (false, false));
    }
}

#[cfg(test)]
mod test_action_tests {
    use super::*;

    #[test]
    fn taps_parse_as_points_with_delays_and_reject_a_bad_entry() {
        let taps = parse_test_taps("200,300@6; 40.5 , 800 @ 9.25").unwrap();
        assert_eq!(taps, vec![(6.0, dvec2(200.0, 300.0)), (9.25, dvec2(40.5, 800.0))]);
        assert_eq!(parse_test_taps("").unwrap(), vec![]);
        let error = parse_test_taps("200,300@6;nonsense").unwrap_err();
        assert!(error.contains("nonsense"), "{error}");
        assert!(parse_test_taps("200,300").is_err(), "a tap needs its delay");
        assert!(parse_test_taps("200@6").is_err(), "a tap needs both coordinates");
    }
}

#[cfg(test)]
mod os_service_tests {
    use super::*;

    #[test]
    fn os_list_reads_as_text_and_as_data() {
        let rows = vec![
            OsAppRow { id: "files".into(), label: "Files".into(), running: true, focused: true },
            OsAppRow { id: "route".into(), label: "Route".into(), running: false, focused: false },
        ];
        let result = os_list_result("c1", &rows);
        assert_eq!(result.outcome, makepad_ai_services::wire::ToolOutcome::Ok);
        assert_eq!(result.text, "Files (`files`) — focused\nRoute (`route`) — not running");
        assert_eq!(result.note, "2 apps, 1 running");
        assert!(result.data.contains(r#""id":"files""#) && result.data.contains(r#""running":true"#));
        assert!(result.data.starts_with('[') && result.data.ends_with(']'));
    }

    #[test]
    fn a_launch_answer_says_running_or_starting() {
        let running = os_launch_answer("c1", "Photos", true);
        assert!(running.text.contains("already running") && running.note == "already running");
        let starting = os_launch_answer("c2", "Photos", false);
        assert!(starting.text.contains("starting") && starting.note == "starting");
    }

    #[test]
    fn bare_keys_have_no_modifier() {
        let none = KeyModifiers { shift: false, control: false, alt: false, logo: false };
        assert!(bare_key(&none));
        assert!(!bare_key(&KeyModifiers { shift: true, ..none }));
        assert!(!bare_key(&KeyModifiers { logo: true, ..none }));
    }
}

#[cfg(all(test, feature = "app-hub", feature = "app-reference"))]
mod app_hub_lifecycle_tests {
    use super::*;
    use makepad_app_module::AppModule;

    #[test]
    fn installing_an_update_closes_only_that_apps_old_instances() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut app = cx.with_vm(App::script_new);
        app.state = Some(WmState {
            snap: Default::default(),
            phone: Default::default(),
            style: Default::default(),
            material: Default::default(),
            roles: Default::default(),
            dock_backdrop: None,
            layout: crate::layout::WmLayout::new(),
            clients: Default::default(),
            hub_port: 0,
            theme_name: String::new(),
            term_env: String::new(),
            accent: Default::default(),
            bar_ground: Default::default(),
            borders: Default::default(),
            gap: desk::TILE_GAP,
            gaps_out: desk::GAPS_OUT,
            dragging: Vec::new(),
            drop_hint: None,
            pane_sliding: false,
        });
        // Real module isolates exercise shutdown and client removal without
        // requiring a downloaded Card or changing the machine's catalog.
        let module = &octosense_reference::REFERENCE_MODULE;
        for (client, id) in [(1, "hub:org.example.timer"), (2, "hub:org.example.timer"),
                             (3, "hub:org.example.notes"), (4, "reference")] {
            let open = module.open_schema().validate("{}", &[]).unwrap();
            app.module_host.create(&mut cx, client, module, open, dvec2(400.0, 800.0)).unwrap();
            app.state_mut().clients.insert(client, clients::ClientSlot::module(client, id, id));
        }
        crate::shell::launcher::apps();
        assert!(crate::shell::launcher::apps_memoized());
        app.installed_app_changed(&mut cx, "org.example.timer");
        assert!(!crate::shell::launcher::apps_memoized(), "the launcher must list the changed library now, not in a second");
        for client in [1, 2] {
            assert!(!app.module_host.is_module(client), "the updated app's old isolate must close");
            assert!(!app.state_mut().clients.contains_key(&client), "Open must not focus the old instance");
        }
        for client in [3, 4] {
            assert!(app.module_host.is_module(client), "other apps must remain running");
            assert!(app.state_mut().clients.contains_key(&client));
            app.module_host.teardown(&mut cx, client);
        }
    }
}

fn super_chord(m: &KeyModifiers) -> bool {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        // Both spellings, exactly like the keymap: ⌘ IS Hyprland's SUPER,
        // and Ctrl+Alt is the fallback for what the host OS eats.
        m.logo || (m.control && m.alt)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        m.logo
    }
}

fn scan_theme_color(source: &str, key: &str) -> Option<Vec4f> {
    for line in source.lines() {
        let line = line.trim();
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        if k.trim() == key {
            let rgb = theme::parse_hex(v.trim())?;
            return Some(Vec4f {
                x: rgb.r as f32 / 255.0,
                y: rgb.g as f32 / 255.0,
                z: rgb.b as f32 / 255.0,
                w: 1.0,
            });
        }
    }
    None
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // Where App Hub keeps what it installs: `$OCTOSENSE_APP_DATA`, else
        // `apps/` in the platform data directory or OctoSense's own state.
        // App storage (ADR 0004 §11): the one source of every app's jail,
        // account folders and secrets; the startup check refuses any agent
        // workspace that reaches the secrets.
        let storage = app_storage::init(cx.get_data_dir().map(std::path::PathBuf::from));
        #[cfg(any(feature = "app-hub", native_mobile))]
        octosense_app_hub_app::set_data_root(storage.map(|s| s.layout().apps_root().to_path_buf())
            .unwrap_or_else(|| cx.get_data_dir().map(std::path::PathBuf::from)
                .unwrap_or_else(octosense::paths::home).join("apps")));
        let _ = storage;
        // The assistant's services (octosense-ai-host): the octos kernel,
        // configured here and started when a consumer (AppCard, Rinx)
        // connects, and AI providers' `llm` service, which writes its profile
        // and restarts it after a change. A no-op where the build links none.
        // Its octos home is OctoSense's own (`<data dir>/octos-home`; on a
        // desktop, OctoSense's state dir), never the person's `~/octos-home`.
        ai_host::start(ai_host::Host::platform(cx.get_data_dir().or_else(|| {
            Some(octosense::paths::home().to_string_lossy().into_owned())
        })));
        // Developer mode (ADR 0004 §13): from this launch's flag or
        // environment, or a developer profile's saved state; before any app
        // starts, so grants and approvals see it from the first call.
        dev_mode::init(&octosense::paths::home());
        octosense::paths::scope_linked_app_data();
        self.dev_generation = dev_mode::generation();
        // Approvals (ADR 0004 §8, §4): this home's standing rules, consent
        // and audit, before any app can ask for an approval.
        approvals::init(&octosense::paths::home());
        // The shell is every app agent's tool host (octos UPCR-2026-035):
        // brokers register app tools and hand their calls, cancels,
        // approvals and the system agent's inputs to `host_tools`.
        host_tools::init();
        // The system agent's grants (Setup > Assistant > Command execution),
        // handed to the kernel before it first starts.
        system_chat::init(std::path::Path::new(&octosense::paths::home()));
        // An agent for every app that declares one (ADR 0004 §4): the ones
        // the person already allowed get their peer now, so the system
        // agent's peer_list shows them.
        agents::start();
        // CLI: --import-theme <name> pulls an omarchy theme and converts
        // it to splash before the desktop appears.
        let mut args = std::env::args();
        while let Some(arg) = args.next() {
            if arg == "--import-theme" {
                if let Some(name) = args.next() {
                    match theme::import_omarchy_theme(&name) {
                        Ok(msg) => log!("wm: {}", msg),
                        Err(err) => log!("wm: import failed: {}", err),
                    }
                }
            }
        }

        let startup_style = octosense::policy::startup_style();
        let theme_name = Self::theme_name_from_env();
        // Children inherit the theme file path so every Makepad app styles
        // itself from the same theme.splash.
        host::set_child_env("MAKEPAD_WM_THEME_SPLASH", theme::theme_splash_path(&theme_name).as_os_str());
        let wallpaper = self.ui.widget(cx, ids!(wallpaper));
        if let Some(mut desk) = self.desk(cx).borrow_mut::<WmDesk>() { desk.wallpaper = wallpaper; }
        let sheet = octosense::style::load_sheet(desktop::DesktopStyle::Omarchy, false);
        host::set_child_env("MAKEPAD_WIDGET_STYLE", std::ffi::OsStr::new(&sheet.name));
        self.module_host.apply_style(cx, &sheet);
        let (material, roles) = Self::chrome_from_sheet(&sheet);
        self.stylesheet = Some(sheet);
        let source = theme::load_theme_source(&theme_name);
        self.warm_pool.set_browser_appearance(desktop_app::browser_appearance(
            desktop::DesktopStyle::Omarchy, false, &source));
        let term_env = theme::scan_term_palette(&source)
            .map(|p| p.env_value())
            .unwrap_or_default();
        let accent = scan_theme_color(&source, "accent").unwrap_or(Vec4f {
            x: 0.48,
            y: 0.63,
            z: 0.97,
            w: 1.0,
        });
        let borders = desk::BorderTheme::from_theme_source(&source);
        // `mod.wm_theme.background` as the DSL resolved it: this theme's
        // own, else the bundled default the DSL fell back to.
        let bar_ground = scan_theme_color(&source, "background")
            .or_else(|| scan_theme_color(theme::BUNDLED_TOKYO_NIGHT_SPLASH, "background"))
            .unwrap_or_default();

        // The client hub is the PROCESS host's: a build without processes
        // (the web) never binds one and hosts its linked modules instead.
        let hub = if host::processes_available() {
            WmHub::start(cx.thread_spawner())
        } else {
            None
        };
        let hub_port = hub.as_ref().map(|h| h.port).unwrap_or(0);
        if hub.is_none() && host::processes_available() {
            log!("wm: could not bind the client hub; tiles will not start");
        }
        self.hub = hub;

        self.state = Some(WmState {
            snap: Default::default(),
            phone: Default::default(),
            layout: crate::layout::WmLayout::new(),
            dock_backdrop: None,
            clients: std::collections::HashMap::new(),
            hub_port,
            theme_name: theme_name.clone(),
            term_env,
            accent,
            bar_ground,
            borders,
            // gaps_in 5 sits on each side of a window, so two tiles are
            // 10 apart — the same as gaps_out to the desk edge.
            gap: desk::TILE_GAP,
            gaps_out: desk::GAPS_OUT,
            dragging: Vec::new(),
            drop_hint: None,
            pane_sliding: false,
            style: Default::default(),
            material,
            roles,
        });
        // The kits start flat; the startup sheet's material still travels
        // the one path a style switch uses.
        self.apply_material_to_chrome(cx, material, None);
        self.next_id = 1;
        // The hosting registry: the linked modules, the person's overrides
        // in ~/.makepad/wm/apps.splash, a dev run's `--module <id>` flags.
        let args: Vec<String> = std::env::args().collect();
        self.apps = AppRegistry::load(&theme::makepad_home().join("wm/apps.splash"), &args);
        log!("wm: modules linked: {:?}", self.apps.linked_ids());
        // Use the normal style-switch path before the first frame, so phone
        // state, controls, icons and hosted-app styles all agree from startup.
        if startup_style != self.state_mut().style.target {
            self.set_desktop_style(cx, startup_style);
            // The first frame starts settled in the selected style; only
            // later user-initiated style switches animate between layouts.
            self.state_mut().style.step(1.0);
        }
        mobile_island::install_producers();
        self.reapprove_hosted_cards(cx);
        if cfg!(native_mobile) {
            self.update_bar_chrome(cx, &WindowGeom {
                safe_area_insets: cx.display_context.safe_area_insets,
                ..Default::default()
            });
        }
        // An in-process assistant: the WM's own service waits on Cx for
        // the pane's root to adopt it, so it is there from the first open.
        if self.apps.pane_in_process() {
            self.pane_links.open_os(cx, AiBus::os_manifest(&Self::registry_apps()));
        }
        self.tick = cx.start_interval(1.0);
        mobile_perf::init_from_env(cx);
        #[cfg(target_os = "android")]
        self.android_command(cx, "launcher", "catalog", vec![]);
        // The warm pool's pump. Started even when the pool is off: it
        // costs one no-op wakeup and keeps the timer id stable.
        if self.warm_pool.enabled() {
            self.warm_tick = cx.start_interval(WARM_PUMP);
        }
        if !self.warm_pool.enabled() {
            log!("octosense: background app prewarming disabled");
        }

        // `--gallery`: the shell surfaces with fixture data instead of a
        // desktop, so the port is verifiable over --remote (shell/gallery.rs).
        if shell::gallery::ShellGallery::requested() {
            self.gallery = true;
            // Built now, not in the DSL: the fixture tree is deep and only
            // a gallery run ever looks at it (shell/gallery.rs).
            let host = self.ui.widget(cx, ids!(shell_gallery_host));
            if let Some(mut host) = host.borrow_mut::<shell::gallery::ShellGalleryHost>() {
                host.ensure(cx);
            }
            self.ui.widget(cx, ids!(gallery_holder)).set_visible(cx, true);
            self.ui.widget(cx, ids!(main_column)).set_visible(cx, false);
            self.redraw_all(cx);
            return;
        }

        if octosense::policy::requested("--assistant") {
            if self.apps.pane_in_process() {
                self.with_ai_pane(cx, |cx, p| p.ensure_overlay(cx));
            } else if hub_port != 0 && clients::find_app("aichat").is_some_and(|a| a.is_available()) {
                self.launch_ai_pane(cx);
            }
        }
        if let Err(error) = octosense::catalog::loaded() {
            log!("octosense: {error}");
            self.notify(cx, "Could not load apps", error);
        }
        for notice in dev_mode::take_notices() {
            self.notify(cx, "Developer mode", &notice);
        }

        // Start EMPTY like omarchy — booting children is slow (first-exec
        // scan) and the desk is usable instantly. MAKEPAD_WM_TEST_APP=app[:count]
        // boots a test scene ("terminal:3" = the old A | (B / C)).
        if let Ok(spec) = std::env::var("MAKEPAD_WM_TEST_APP") {
            let (app, count) = match spec.split_once(':') {
                Some((app, n)) => (app.to_string(), n.parse::<usize>().unwrap_or(1).min(9)),
                None => (spec, 1),
            };
            for _ in 0..count {
                self.launch_app(cx, &app);
            }
        }

        if startup_style == desktop::DesktopStyle::Omarchy {
            self.apply_background(cx, 0);
        }
        if octosense::policy::requested("--download-wallpapers") {
            self.fetch_backgrounds_if_missing(cx);
        }
        self.update_bar(cx);
        self.update_status(cx);
        // The platform installs a default menu whose Quit is ⌘Q — which
        // would take the whole desktop down when the user means "close
        // this window" (omarchy's SUPER+Q). Replace it: Quit keeps a key
        // equivalent no window binding uses.
        #[cfg(target_os = "macos")]
        cx.update_macos_menu(MacosMenu::Main {
            items: vec![MacosMenu::Sub {
                name: "OctoSense".to_string(),
                items: vec![MacosMenu::Item {
                    command: live_id!(quit),
                    key: KeyCode::KeyQ,
                    shift: true,
                    enabled: true,
                    name: "Quit OctoSense".to_string(),
                }],
            }],
        });

        // OCTOSENSE_GLANCE_DEMO=1: a sample News digest on the glance screen.
        glance::publish_demo_if_asked();
        // Scripted verification: --test-action fires WM actions with no
        // keyboard involved (some chords belong to the host OS).
        self.run_test_actions(cx);
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.gallery {
            let gallery = self.ui.widget(cx, ids!(shell_gallery));
            {
                let mut borrowed = gallery.borrow_mut::<shell::gallery::ShellGallery>();
                if let Some(g) = borrowed.as_mut() {
                    g.handle_actions(cx, actions);
                }
            }
        }
        for action in actions {
            let Some(wa) = action.as_widget_action() else {
                continue;
            };
            if let ModuleCloseAction::Confirmed = wa.cast::<ModuleCloseAction>() {
                self.module_close_confirmed(cx, wa.widget_uid);
            }
            match wa.cast::<glance_panel::ShellGlancePanelAction>() {
                glance_panel::ShellGlancePanelAction::None => {}
                action => self.glance_panel_action(cx, action),
            }
            // A card's notification opens that card in the card window.
            match wa.cast::<shell::notifications::ShellNotificationsAction>() {
                shell::notifications::ShellNotificationsAction::Activated(id) if self.glance_toasts.contains(id) => {
                    if let Some(key) = self.glance_toasts.activated(id) {
                        self.open_glance_card(cx, &key);
                    }
                }
                shell::notifications::ShellNotificationsAction::Dismissed(id) => self.glance_toasts.dismissed(id),
                // The glance panel's Undo.
                shell::notifications::ShellNotificationsAction::Action(id) if self.glance_undo_toast == Some(id) => self.glance_undo(cx),
                _ => {}
            }
            #[cfg(any(feature = "app-hub", native_mobile))]
            match wa.cast::<octosense_app_hub_app::AppHubAction>() {
                octosense_app_hub_app::AppHubAction::Launch(id) => self.launch_app(cx, &id),
                octosense_app_hub_app::AppHubAction::OpenInstalled(id) => self.launch_app(cx, &apps::installed_launch_id(&id)),
                octosense_app_hub_app::AppHubAction::None => {}
            }
            // The shell surfaces: the bar's presses and wheel, the menu's
            // activations, the flyouts' controls.
            match wa.cast::<ShellBarAction>() {
                ShellBarAction::Press(module) => match module {
                    #[cfg(not(mobile_only))]
                    BarModule::Appearance => self.toggle_desktop_appearance(cx),
                    #[cfg(not(mobile_only))]
                    BarModule::Style => self.open_style_menu(cx),
                    BarModule::Menu => self.toggle_launcher(cx),
                    BarModule::Workspace(i) => {
                        let ws = self.bar_workspaces.get(i).copied().unwrap_or(i);
                        self.do_action(cx, WmAction::Workspace(ws));
                    }
                    BarModule::ActiveWindow => {
                        if let Some(focus) = self.state_mut().layout.focused_client() {
                            self.focus_client(cx, focus);
                        }
                    }
                    BarModule::AskAgent => self.ask_focused_app(cx),
                    BarModule::Glance => {
                        let open = self.glance_open(cx);
                        self.set_glance_open(cx, !open);
                    }
                    control @ (BarModule::WindowMin | BarModule::WindowMax | BarModule::WindowClose) => {
                        // The gallery's bar is a picture of one: its
                        // controls log (shell/gallery.rs), never close or
                        // shrink the window they are drawn in.
                        if !self.gallery {
                            self.window_control(cx, control);
                        }
                    }
                    other => self.toggle_shell_panel(cx, other),
                },
                ShellBarAction::RightPress(module) => match module {
                    // The Omarchy button's right click opens a terminal.
                    #[cfg(not(mobile_only))]
                    BarModule::Appearance => self.toggle_desktop_appearance(cx),
                    #[cfg(not(mobile_only))]
                    BarModule::Style => self.open_style_menu(cx),
                    BarModule::Menu => self.do_action(cx, WmAction::LaunchTerminal),
                    BarModule::ActiveWindow => {
                        if let Some(focus) = self.state_mut().layout.focused_client() {
                            self.request_close(cx, focus);
                        }
                    }
                    BarModule::Clock => {
                        self.clock_alt = !self.clock_alt;
                        self.update_status(cx);
                        self.update_bar(cx);
                    }
                    BarModule::Audio => {
                        self.bar_sample.muted = !self.bar_sample.muted;
                        #[cfg(target_os = "macos")]
                        {
                            let _ = std::process::Command::new("osascript")
                                .args([
                                    "-e",
                                    &format!(
                                        "set volume output muted {}",
                                        self.bar_sample.muted
                                    ),
                                ])
                                .output();
                        }
                        self.update_bar(cx);
                    }
                    _ => {}
                },
                ShellBarAction::MiddlePress(module) => {
                    // `ActiveWindow.qml`: middle click closes the focused
                    // window exactly like a right click — every other
                    // module ignores the middle button.
                    if module == BarModule::ActiveWindow {
                        if let Some(focus) = self.state_mut().layout.focused_client() {
                            self.request_close(cx, focus);
                        }
                    }
                }
                ShellBarAction::Wheel(module, dir) => {
                    if module == BarModule::Audio {
                        let level = self.bar_sample.volume.unwrap_or(0) as f64;
                        let next = (level + dir * 5.0).clamp(0.0, 100.0) as u32;
                        self.set_volume(cx, next);
                    }
                }
                ShellBarAction::None => {}
            }
            // ONLY the overlay's own menu instance may drive the desktop —
            // the gallery embeds fixture menus whose actions must never
            // launch apps or open the real card (the ghost-launch bug).
            if wa.widget_uid == self.ui.widget(cx, ids!(shell_menu)).widget_uid() {
                match wa.cast::<ShellMenuAction>() {
                    ShellMenuAction::Activate(target) => self.shell_menu_activate(cx, &target),
                    ShellMenuAction::Cancel => self.close_shell_menu(cx),
                    ShellMenuAction::None => {}
                }
            }
            match wa.cast::<ShellPanelAction>() {
                ShellPanelAction::SetVolume(v) => self.set_volume(cx, v),
                ShellPanelAction::ToggleMute => {
                    self.bar_sample.muted = !self.bar_sample.muted;
                    self.update_bar(cx);
                }
                ShellPanelAction::Close => {
                    self.shell_panel_open = None;
                    self.update_bar(cx);
                }
                _ => {}
            }
            // A widget action only the package answers (the phone's
            // `SettingsRequest`) is left for it: it reads the same actions
            // after the shell (phone/src/main.rs).
            // A module root asking the window manager: the request is a
            // widget action posted from inside its isolate, attributed by
            // the root's uid and handled exactly as a process's would be.
            if let Some(req) = wa.action.downcast_ref::<WmRequest>() {
                if let Some(client) = self.module_host.client_of_root_uid(wa.widget_uid) {
                    let req = req.clone();
                    self.on_wm_request(cx, client, req);
                    continue;
                }
                // A child widget posting instead of its root is attributed
                // to nobody: say so rather than lose the request silently.
                log!("wm: WmRequest from widget {:?} matches no module root", wa.widget_uid);
            }
            match wa.cast::<MpRunViewAction>() {
                MpRunViewAction::ForwardToApp { client, msg_bin } => {
                    if let Some(state) = self.state.as_ref() {
                        if let Some(slot) = state.clients.get(&client) {
                            if let Some(sender) = &slot.sender {
                                let _ = sender.send(msg_bin);
                            }
                        }
                    }
                }
                MpRunViewAction::Clicked { client } => {
                    self.focus_client(cx, client);
                }
                MpRunViewAction::Restart { client } => self.restart_failed_module(cx, client),
                MpRunViewAction::None => {}
            }
            match wa.cast::<WmDeskAction>() {
                WmDeskAction::TearOutTab { client, abs } => self.tear_out_tab(cx, client, abs),
                WmDeskAction::None => {}
            }
        }
    }

    fn handle_signal(&mut self, cx: &mut Cx) {
        if self.state.is_some() {
            #[cfg(any(feature = "app-hub", native_mobile))]
            for id in octosense_app_hub_app::take_completed_installs() {
                self.installed_app_changed(cx, &id);
                // Its tools, grants and kernel tools, as installed (ADR 0004 §7).
                host_tools::script_app_installed(&id);
            }
            self.drain_hub(cx);
            self.drain_client_lines(cx);
            self.drain_module_upstream();
            self.drain_module_windows(cx);
        }
    }
}

/// The shell's half of `AppMain`: a package with work of its own (the
/// phone's Settings) wraps these in its own `App`; the desktop runs them
/// as they are (`impl AppMain for App` below).
impl App {
    pub fn shell_script_mod(vm: &mut ScriptVm) -> ScriptValue {
        host::set_child_env("MAKEPAD_HOME", octosense::paths::home().as_os_str());
        desktop_style::install(vm,desktop_style::StyleSheet::load(desktop_style::DesktopStyle::Omarchy));
        crate::makepad_widgets::script_mod(vm);
        octosense::icon_frame::script_mod(vm);

        // The theme: evaluated before any module that reads
        // mod.wm_theme. This IS the theming system — splash.
        theme::ensure_default_theme();
        let theme_name = App::theme_name_from_env();
        let source = theme::load_theme_source(&theme_name);
        let eval_theme = |vm: &mut ScriptVm, name: &str, code: &str| -> bool {
            let script_mod_id = ScriptMod {
                cargo_manifest_path: env!("CARGO_MANIFEST_DIR").to_string(),
                module_path: name.to_string(),
                file: "theme.splash".to_string(),
                line: 0,
                column: 0,
                code: code.to_string(),
                values: vec![],
            };
            let value = vm.eval(script_mod_id);
            let errors = vm.take_errors();
            for e in &errors {
                log!("wm theme: {}", e);
            }
            !value.is_err() && errors.is_empty()
        };
        // Leading comment lines shift the runtime parser's span tracking
        // (the script_mod! gotcha applies to eval bodies too): start the
        // body at the first real statement.
        let source: String = {
            let mut lines = source.lines().peekable();
            while let Some(line) = lines.peek() {
                let t = line.trim();
                if t.is_empty() || t.starts_with("//") {
                    lines.next();
                } else {
                    break;
                }
            }
            let mut s = lines.collect::<Vec<_>>().join("\n");
            // Parser quirk: the FINAL statement of an eval body is treated
            // as its result expression — a trailing `mod.x = {...}` never
            // commits. A benign trailing statement makes the assignment a
            // real statement. (Same class as the splash auto-close notes.)
            s.push_str("\ntrue\n");
            s
        };
        if !eval_theme(vm, "wm_theme", &source) {
            // Fall back to the bundled default so the DSL still evaluates.
            let mut fallback = theme::BUNDLED_TOKYO_NIGHT_SPLASH.to_string();
            fallback.push_str("\ntrue\n");
            eval_theme(vm, "wm_theme_fallback", &fallback);
        }

        // The shell token object (`mod.wm_theme.shell`): the omarchy
        // `shell.toml.tpl` contract, resolved from this theme's palette —
        // unless the theme ships its own `shell: {...}` block, which
        // replaces it wholesale.
        if !theme::theme_defines_shell(&source) {
            let mut block = theme::shell_splash_block(&source);
            block.push_str("\ntrue\n");
            eval_theme(vm, "wm_theme_shell", &block);
        }

        run_view::script_mod(vm);
        module_view::script_mod(vm);
        // The assistant as a module: its panel and overlay root, so the
        // pane can seat `mod.widgets.AiChatOverlay{}` by name.
        #[cfg(feature = "app-aichat")]
        makepad_aichat::script_mod(vm);
        shell::script_mod(vm);
        glance_card::script_mod(vm);
        glance_panel::script_mod(vm);
        card_chat::script_mod(vm);
        glance_sheet::script_mod(vm);
        approvals::script_mod(vm);
        system_chat::script_mod(vm);
        desktop::script_mod(vm);
        snap::script_mod(vm);
        mobile_surface::script_mod(vm);
        desk::phone::script_mod(vm);
        dock_warp::script_mod(vm);
        desk::script_mod(vm);
        scene::script_mod(vm);
        self::script_mod(vm)
    }

    pub fn shell_handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.webview_render.handle_event(cx, event);
        self.shell_handle_event_inner(cx, event);
        // Whatever module panicked during this event — in its tile's event
        // or draw, or in a call the shell made — is contained by now; show
        // it closed and free it before the next event (module_host.rs).
        self.contain_module_faults(cx);
        // Modules' peer links (#142): the links opened during this event,
        // and what the instances sent on theirs, to the shell's peer link.
        self.module_host.pump_peer_links(cx);
        // A quit that waited on instances asking the person goes ahead once
        // the last of them confirmed (or failed and has nothing left to ask).
        if self.take_quit_ready() {
            log!("wm: every instance and app confirmed; quitting");
            cx.quit();
        }
    }

    fn shell_handle_event_inner(&mut self, cx: &mut Cx, event: &Event) {
        // Quitting, or closing the shell's window, asks the hosted instances
        // first (makepad#65). A termination signal is never refused.
        match event {
            Event::QuitRequested(request)
                if request.reason != QuitReason::Signal && !request.handled.get() && !self.ask_before_quit(cx) =>
            {
                request.handle();
            }
            Event::WindowCloseRequested(request) if request.accept_close.get() && !self.ask_before_quit(cx) => {
                request.accept_close.set(false);
            }
            _ => {}
        }
        // Recording belongs to the WM, including on Home and in an OS menu.
        // Forwarding this chord also starts a recorder in the focused child.
        if let Event::KeyDown(e) | Event::KeyUp(e) = event {
            if e.key_code == KeyCode::F10 && e.modifiers.control && !e.modifiers.shift {
                if matches!(event, Event::KeyDown(_)) && !e.is_repeat {
                    if let Some(mut window) = self.ui.window(cx, ids!(main_window)).borrow_mut() {
                        window.toggle_recording(cx);
                    }
                }
                return;
            }
        }
        mobile_perf::saw_event(event);
        // The assistant's host-owned surfaces: a QR scanner or image picker
        // AI providers asked for, and their answers.
        ai_host::handle_event(cx, event);
        if self.android_event(cx, event) { return; }
        // Android's Home button or gesture, with OctoSense as the Home app.
        if matches!(event, Event::HomeIntent) { self.phone_home_intent(cx); return; }
        self.phone_animation_event(cx,event);
        // Android's Back key (and a platform Back of any kind) is the phone's
        // Back: the foreground app is offered it first (mobile_back.rs), so it
        // is not also broadcast through the widget tree.
        if event.back_pressed() && self.state.as_ref().is_some_and(|state| state.style.target.mobile()) {
            log!("[phone] back");
            if self.close_glance_card(cx) { return; }
            // An open assistant pane takes Back next, as its own Close does.
            if self.close_chat_panes(cx, false) { return; }
            self.phone_action(cx, mobile::PhoneHit::Back);
            return;
        }
        if let Some(ne) = self.style_frame.is_event(event) {
            if self.state.is_some() {
                let dt=if self.style_time==0.0 {0.0}else{(ne.time-self.style_time).min(0.05)};
                self.style_time=ne.time;
                if self.state_mut().style.step(dt) {self.style_frame=cx.new_next_frame();}
                self.redraw_all(cx);
            }
        }
        if let Event::WindowGeomChange(ev) = event {
            if ev.old_geom.inner_size != ev.new_geom.inner_size {
                if let Some(mut scene) = self.ui.widget(cx, ids!(scene)).borrow_mut::<scene::WmScene>() {scene.cut(cx);}
            }
            // The bar is the caption: keep it around the OS buttons.
            self.update_bar_chrome(cx, &ev.new_geom);
            // A warm instance is configured with the desktop's own dpi, so
            // what it warms up is what its tile will use.
            self.dpi_factor = ev.new_geom.dpi_factor;
        }
        if let Event::WindowDragQuery(dq) = event {
            // Caption-less window: the bar strip is the drag handle; the
            // desk below answers Client so tile clicks never move the
            // window. macOS treats unanswered strip points as native drag.
            // The shell bar's own modules are BUTTONS, not a drag handle:
            // where it claims a point, the press reaches the widget.
            let bar = self.ui.view(cx, ids!(bar)).area();
            if cfg!(native_mobile) || MOBILE_ONLY
                || self.phone_toolbar_hit(cx,dq.abs).is_some() || self.shell_bar_claims(cx, dq.abs) {
                dq.response.set(WindowDragQueryResponse::Client);
            } else if bar.is_valid(cx) && bar.rect(cx).contains(dq.abs) {
                dq.response.set(WindowDragQueryResponse::Caption);
            } else {
                dq.response.set(WindowDragQueryResponse::Client);
            }
        }
        #[cfg(not(mobile_only))]
        if self.state.is_some() && !self.state_mut().style.target.mobile() {
            if let Event::MouseDown(e)=event {
                let module=self.ui.widget(cx,ids!(shell_bar)).borrow::<shell::bar::ShellBar>().and_then(|b|b.module_at(e.abs));
                if module==Some(BarModule::Appearance) {self.toggle_desktop_appearance(cx);return;}
                if module==Some(BarModule::Style) {
                    self.open_style_menu(cx);
                    return;
                }
            }
        }
        // An approval sheet, the first-use sheet and the Approvals page
        // are modal, over everything (approvals/mod.rs `pointer`).
        if self.state.is_some() && approvals::pointer(&self.ui, cx, event) {
            self.approvals_changed(cx);
            return;
        }
        // The shell menu (and, for move/down/up, an open bar flyout) is
        // modal: while it is up, the pointer event is exclusively its own
        // — see `shell_menu_pointer` / `shell_panel_pointer`. Taken before
        // anything else, including SUPER+drag, so an open surface always
        // wins over the desk beneath it.
        if self.state.is_some()
            && matches!(
                event,
                Event::TouchUpdate(_) | Event::MouseMove(_) | Event::MouseDown(_) | Event::MouseUp(_) | Event::Scroll(_)
            )
            && (self.dev_banner_pointer(cx, event)
                || self.shell_menu_pointer(cx, event)
                || self.shell_panel_pointer(cx, event))
        {
            return;
        }
        // A toast is drawn over the card window, the glance panel, the chat
        // panes and the windows: a press on it is the toast's alone (it
        // opens its card), whatever lies under it.
        if self.state.is_some() && self.press_on_toast(cx, event) {
            self.ui.widget(cx, ids!(shell_notes)).handle_event(cx, event, &mut Scope::empty());
            return;
        }
        if self.state.is_some()
            && matches!(
                event,
                Event::TouchUpdate(_) | Event::MouseMove(_) | Event::MouseDown(_) | Event::MouseUp(_) | Event::Scroll(_)
            )
            && (self.glance_sheet_pointer(cx, event)
                || self.shell_glance_pointer(cx, event)
                || self.app_chat_pointer(cx, event)
                || self.system_chat_pointer(cx, event))
        {
            return;
        }
        // AI providers' QR import on a desktop: an image dropped on its
        // window while the import sheet waits.
        if self.state.is_some() && matches!(event, Event::Drag(_) | Event::Drop(_)) {
            let desk = self.desk(cx);
            let desk = desk.borrow::<WmDesk>();
            let state = self.state.as_ref();
            let app_at = |p: Vec2d| {
                let client = desk.as_ref()?.window_at(p)?;
                state?.clients.get(&client).map(|slot| slot.app.clone())
            };
            if ai_host::handle_drop(event, &app_at) {
                return;
            }
        }
        // Phone Glance is a preview. A plain body tap opens the exact card
        // in its dedicated workspace; preview editors never claim focus.
        // Background events still refresh previews while no workspace covers them.
        if self.state.is_some() && self.state_mut().style.target.mobile() {
            let claimed = self.phone_gestures.current().is_some();
            let phone = &self.state_mut().phone;
            let showing = phone.screen == mobile::PhoneScreen::Home && phone.pages.on_glance() && !phone.shade.is_open() && !phone.card_open;
            let card_open = phone.card_open;
            let finger = mobile_pages::GlanceFinger::of(event, phone.gesture.as_ref(), claimed, phone.touch);
            if showing {
                if let mobile_pages::GlanceFinger::Tap(at) = finger {
                    let key = self.desk(cx).borrow::<WmDesk>().and_then(|desk| desk.phone_ui.glance_cards.under(at).map(str::to_owned));
                    if let Some(key) = key {
                        // Finish the Home gesture, then promote the exact publication.
                        // Preview controls never edit or dispatch inside the feed.
                        self.phone_pointer(cx, event);
                        self.open_glance_card(cx, &key);
                        return;
                    }
                }
            }
            let preview_input = matches!(event, Event::TouchUpdate(_) | Event::MouseDown(_) | Event::MouseMove(_) | Event::MouseUp(_) | Event::Scroll(_) | Event::KeyDown(_) | Event::KeyUp(_) | Event::TextInput(_) | Event::TextCopy(_) | Event::TextCut(_));
            if !preview_input && !card_open {
                let changed = self.desk(cx).borrow_mut::<WmDesk>().is_some_and(|mut desk| desk.phone_ui.glance_cards.handle_event(cx, event, finger));
                if changed {
                    self.redraw_all(cx);
                }
            }
        }
        if self.phone_search_event(cx,event) {return;}
        if self.state.is_some() && self.phone_pointer(cx,event) {return;}
        if self.state.is_some() && self.snap_event(cx,event) {return;}
        if self.state.is_some() && self.desktop_pointer(cx,event) {return;}
        // The AI pane owns the pointer inside its rect while it is open:
        // the event goes to the pane alone, so neither the WM's own drag
        // gestures nor the tile underneath ever see it.
        if self.state.is_some() && self.ai_pane_pointer(cx, event) {
            return;
        }
        // The pane rect follows the desk: refreshed from the desk's last
        // drawn rect right before every draw, one borrow's worth.
        if let Event::Draw(_) = event {
            if self.state.is_some() {
                self.sync_ai_pane_geometry(cx);
            }
        }
        // SUPER + mouse:272 / mouse:273 — move and resize (tiling.lua).
        // Taken before the tiles see it, so the drag never reaches a child.
        if self.state.is_some() && !self.state_mut().style.target.mobile() {
            match event {
                Event::MouseDown(e) if super_chord(&e.modifiers) => {
                    let resize = e.button.contains(MouseButton::SECONDARY);
                    if self.begin_drag(cx, e.abs, resize) {
                        return;
                    }
                }
                Event::MouseMove(e) if self.drag.is_some() => {
                    self.drag_move(cx, e.abs, e.modifiers.shift);
                    return;
                }
                Event::MouseUp(e) if self.drag.is_some() => {
                    self.end_drag(cx, e.abs, e.modifiers.shift);
                    return;
                }
                // A PLAIN press in the gap between two tiles grabs the
                // divider there (`resize_on_border` scoped to the gap —
                // see `DividerDrag`). It only ever fires where nothing
                // else wants the press: over a window `begin_divider_drag`
                // declines and the tiles get it, unchanged.
                Event::MouseDown(e)
                    if !super_chord(&e.modifiers)
                        && e.button.contains(MouseButton::PRIMARY)
                        && self.drag.is_none() =>
                {
                    if self.begin_divider_drag(cx, e.abs) {
                        return;
                    }
                }
                Event::MouseMove(e) if self.div_drag.is_some() => {
                    self.divider_drag_move(cx, e.abs);
                    return;
                }
                Event::MouseUp(_) if self.div_drag.is_some() => {
                    self.end_divider_drag(cx);
                    return;
                }
                // SUPER + wheel over the desk cycles workspaces.
                Event::Scroll(e) if super_chord(&e.modifiers) => {
                    if e.scroll.y.abs() > 0.5 {
                        self.scroll_workspace(cx, e.scroll.y > 0.0);
                    }
                    return;
                }
                _ => {}
            }
        }
        // WM keybinds intercept before anything reaches the tiles.
        if let Event::KeyDown(e) = event {
            if self.state.is_some() {
                // The card window is modal: its card has the keyboard, and
                // Esc closes it.
                let sheet = self.ui.widget(cx, ids!(shell_glance_sheet));
                if sheet.borrow::<glance_sheet::ShellGlanceSheet>().is_some_and(|s| s.is_open()) {
                    self.alt_armed = false;
                    sheet.handle_event(cx, event, &mut Scope::empty());
                    self.redraw_all(cx);
                    return;
                }
                // The shell menu grabs the keyboard while it is up.
                if self.shell_menu_key(cx, e) {
                    self.alt_armed = false;
                    return;
                }
                // The glance panel, while it holds the keyboard (the person
                // opened it or pressed in it last): its arrows, Return,
                // Delete, ⌘Z and Esc, before a chat pane's prompt.
                if self.glance_key(cx, e) {
                    self.alt_armed = false;
                    return;
                }
                // "Ask <app>"'s prompt, while its pane has the keyboard.
                if app_chat::key(e) {
                    self.alt_armed = false;
                    self.system_chat_changed(cx);
                    return;
                }
                // Shift+F8: ask the focused app's agent.
                if e.key_code == KeyCode::F8 && e.modifiers.shift && !e.modifiers.logo && !e.modifiers.control && !e.modifiers.alt {
                    self.alt_armed = false;
                    self.ask_focused_app(cx);
                    return;
                }
                // The system chat's prompt, while its pane is open.
                if system_chat::key(e) {
                    self.alt_armed = false;
                    self.system_chat_changed(cx);
                    return;
                }
                // F8: the system chat, everywhere.
                if e.key_code == KeyCode::F8 && bare_key(&e.modifiers) {
                    self.alt_armed = false;
                    system_chat::toggle();
                    // The system chat takes the keyboard when it opens.
                    app_chat::focus(!system_chat::is_open());
                    self.focus_system_chat(cx);
                    self.system_chat_changed(cx);
                    return;
                }
                if self.phone_key(cx,e) {return;}
                // F10 is the assistant, everywhere (aicontrol decision 8):
                // under the WM the bare key opens the pane for whatever is
                // focused, before the keymap and before any tile.
                if e.key_code == KeyCode::F10 && bare_key(&e.modifiers) {
                    self.alt_armed = false;
                    self.do_action(cx, WmAction::ToggleAi);
                    return;
                }
                // F9: the glance panel (published cards), on a desktop style.
                if e.key_code == KeyCode::F9 && bare_key(&e.modifiers) && !self.state_mut().style.target.mobile() {
                    self.alt_armed = false;
                    self.do_action(cx, WmAction::ToggleGlance);
                    // Opened from the keyboard: its first card has the focus.
                    if let Some(mut panel) = self.ui.widget(cx, ids!(shell_glance)).borrow_mut::<glance_panel::ShellGlancePanel>() {
                        if panel.open {
                            panel.focus_first();
                        }
                    }
                    return;
                }
                if self.desktop_key(cx,e) {return;}
                let armed = self.alt_armed;
                if let Some(action) = match_bind_armed(e.key_code, &e.modifiers, armed) {
                    // Any key but the prefix itself disarms it.
                    if action != WmAction::ArmAltLayer {
                        self.alt_armed = false;
                    }
                    self.do_action(cx, action);
                    return;
                }
                if armed {
                    self.alt_armed = false;
                }
                // A Quick-Look preview closes on Escape or Space, before
                // the key reaches the viewer — unless the pane is up and
                // holds the keyboard: Escape and Space are the chat's then.
                if matches!(e.key_code, KeyCode::Escape | KeyCode::Space)
                    && !self.ai_pane_is_open(cx)
                    && self.close_focused_preview(cx)
                {
                    return;
                }
            }
        }
        if let Event::Shutdown = event {
            if let Some(state) = &mut self.state {
                clients::shutdown_clients(&mut state.clients);
            }
            // Stop the octos kernel, if one runs, and let it release its data dir.
            ai_host::shutdown();
        }
        if let Event::Timer(te) = event {
            self.fire_test_timers(cx, te);
            if self.tick.is_timer(te).is_some() && self.state.is_some() {
                self.reap_exited(cx);
                self.poll_backgrounds(cx);
                self.drain_client_lines(cx);
                self.explain_first_exec_scan(cx);
                self.update_status(cx);
                self.update_bar(cx);
                self.phone_tick(cx);
                dev_mode::tick();
                self.dev_mode_changed(cx);
                self.approvals_tick(cx);
                // The pool fills itself here: at startup, after an
                // adoption, and after any death it healed from. One spawn
                // per second, so a cold desktop never forks four cargo
                // builds at once.
                if !self.gallery {
                    self.top_up_warm_pool(cx);
                }
            }
            if self.warm_tick.is_timer(te).is_some() && self.state.is_some() {
                self.pump_warm(cx);
            }
        }
        // The chat panes' prompts: text input and the input method (see
        // system_chat/composer.rs). A pane whose prompt holds the key focus
        // takes its text and answers the input method's state query; plain
        // typed text (a desktop's) goes to the pane that has the keyboard.
        // Characters are typed only here, never on KeyDown.
        if self.state.is_some() && matches!(event, Event::TextInput(_) | Event::TextInputStateQuery(_) | Event::ImeAction(_)) && self.chat_text_input(cx, event) {
            self.system_chat_changed(cx);
            return;
        }
        // Copy and cut in a chat pane's prompt: its selection.
        if self.state.is_some() && matches!(event, Event::TextCopy(_) | Event::TextCut(_)) && self.chat_clipboard(cx, event) {
            self.system_chat_changed(cx);
            return;
        }
        // The remote's `/event?data=system-call:<tool> <json>`: a test call
        // from the system agent to a granted read tool (host_tools.rs
        // `system_call_test`), logged with its reply.
        if let Event::Custom(data) = event {
            if let Some(spec) = data.strip_prefix("system-call:") {
                host_tools::system_call_test(spec);
                self.host_tools_pump(cx);
                return;
            }
        }
        if let Event::Signal = event {
            if self.state.is_some() {
                self.system_chat_changed(cx);
                // Review requested from Glance or the native Mail composer:
                // always use the same host-owned region in the card sheet.
                if let Some(key) = mail_card::requested_card() {
                    let already_open = self.ui.widget(cx, ids!(shell_glance_sheet))
                        .borrow::<glance_sheet::ShellGlanceSheet>()
                        .is_some_and(|s| s.open_key() == Some(key.as_str()));
                    if !already_open { self.open_glance_card(cx, &key); }
                    self.redraw_all(cx);
                }
            }
            // A card was published, replaced or withdrawn (glance.rs).
            if glance::generation() != self.glance_generation && self.state.is_some() {
                self.glance_generation = glance::generation();
                // A new card opens the glance panel on a desktop, unless a
                // card window is up; the bar's glance button counts the cards.
                let fresh = glance::listed().iter().any(|c| c.published_ms > self.glance_seen_ms);
                let sheet_open = self.ui.widget(cx, ids!(shell_glance_sheet)).borrow::<glance_sheet::ShellGlanceSheet>().is_some_and(|s| s.is_open());
                if fresh && !sheet_open && !self.state_mut().style.target.mobile() && !self.glance_open(cx) {
                    log!("wm: a new glance card opens the glance panel");
                    if let Some(mut panel) = self.ui.widget(cx, ids!(shell_glance)).borrow_mut::<glance_panel::ShellGlancePanel>() {
                        panel.show_newest();
                    }
                    self.set_glance_open(cx, true);
                }
                self.update_bar(cx);
                self.redraw_all(cx);
            }
            if self.state.is_some() {
                // A card that asked to notify gets its toast even when the
                // panel opens with it: the toasts stack clear of the panel
                // (notifications.rs `keep_clear_of`).
                for note in glance::take_notifications() {
                    self.glance_notify(cx, &note);
                }
            }
            if SignalToUI::check_and_clear_ui_signal() && self.state.is_some() {
                crate::run_view::trace_host("sig");
                self.poll_backgrounds(cx);
                self.drain_hub(cx);
                self.drain_client_lines(cx);
            }
        }

        // An in-process assistant's calls arrive on channels, not events:
        // looked at on every event, cheap when there is nothing.
        if self.state.is_some() {
            self.drain_pane_links(cx);
        }

        let focused_card = self.ui.widget(cx, ids!(shell_glance_sheet)).borrow::<glance_sheet::ShellGlanceSheet>().is_some_and(|s| s.is_open());
        if let Some(state) = self.state.as_mut() { state.phone.card_open = focused_card && state.style.target.mobile(); }
        self.match_event(cx, event);
        if let Some(state) = self.state.as_mut() {
            let mut scope = Scope::with_data(state);
            self.ui.handle_event(cx, event, &mut scope);
        } else {
            self.ui.handle_event(cx, event, &mut Scope::empty());
        }
        // A focus that couldn't land at launch (tile not yet drawn) is
        // re-asserted for a process tile when its first frame arrives
        // (PresentableDraw). A module tile sends no frame, so retry after the
        // draw that may have created it; nothing forces a redraw, so a tile
        // that still cannot take it just tries again on the next draw.
        if let (Event::Draw(_), Some(client)) = (event, self.pending_focus) {
            if self.module_host.is_module(client) {
                let focused = self
                    .desk(cx)
                    .borrow_mut::<WmDesk>()
                    .and_then(|mut d| d.with_tile(cx, client, |cx, v| v.focus_keyboard(cx)))
                    .unwrap_or(false);
                if focused {
                    self.pending_focus = None;
                    self.keyboard_holder = Some(client);
                }
            }
        }
        self.sync_phone_keyboard(cx);
        // Style reloads and phone capture teardown may retire draw lists during
        // this event. Remove their pass roots before upstream scans GPU demand.
        octosense::retired_passes::clear_retired_roots(cx);
        // The gap cursor, LAST: a tile hover-out inside `ui.handle_event`
        // resets the cursor to Default, and the frame's final `set_cursor`
        // is the one the platform applies.
        if let Event::MouseMove(e) = event {
            self.update_divider_cursor(cx, e.abs);
        }
    }
}

/// The desktop package's `AppMain`: the shell alone.
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        App::shell_script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.shell_handle_event(cx, event);
    }
}

/// A package's entry point: `use octosense_shell::makepad_widgets::*;`, an
/// `App` in scope (the shell's, or the package's wrapper), then
/// `octosense_shell::octosense_main!();` — the fonts every shell ships, and
/// the package's own directory (its catalog, `upstream/`, Cargo.toml) for
/// `octosense::paths::project_root`.
#[macro_export]
macro_rules! octosense_main {
    ($($extra:literal),* $(,)?) => {
app_main!(
    App,
    font_set: International,
    font_assets: [
        "makepad_widgets/resources/jetbrains_mono_variable.ttf",

        "makepad_widgets/resources/NotoColorEmoji.ttf",
        // The faces the AppCard module's L0 kit names by file
        // (`crate_resource("makepad_widgets:resources/<face>.ttf")`): the
        // hero's hairline Roboto-Thin, the label weights, the geometric
        // and serif roles. A face the package lacks draws NOTHING.
        "makepad_widgets/resources/Roboto-Thin.ttf",
        "makepad_widgets/resources/Roboto-Light.ttf",
        "makepad_widgets/resources/Roboto-Regular.ttf",
        "makepad_widgets/resources/Roboto-Medium.ttf",
        "makepad_widgets/resources/Roboto-Bold.ttf",
        "makepad_widgets/resources/Montserrat-Regular.ttf",
        "makepad_widgets/resources/Montserrat-Medium.ttf",
        "makepad_widgets/resources/Montserrat-SemiBold.ttf",
        "makepad_widgets/resources/Serif-Regular.ttf",
        "makepad_widgets/resources/Serif-Bold.ttf",
        $($extra),*
    ],
    configure: |_cx: &mut Cx| {
        $crate::octosense::paths::set_package_dir(env!("CARGO_MANIFEST_DIR"));
    }
);
    };
}

/// The person's home directory (`HOME`; `USERPROFILE` on Windows), where a
/// new terminal with no directory to inherit opens.
fn user_home() -> Option<std::path::PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).filter(|home| !home.is_empty()).map(std::path::PathBuf::from).filter(|home| home.is_dir())
}
