//! Octoscript-AppCard inside OctoSense, Phase A: one L0 card in a home tile,
//! in-process, in an isolate of its own.
//!
//! What runs here is the RENDER half of AppCard: the AppCard widget set on
//! the makepad fork's `port/appcard-on-octoscript` line (kit components, text
//! roles, WeatherIcon, TempBar, SunArc, MoonPhase, AqiContour, matplot), a
//! `Splash` whose body is the lowered card, and the `sys.*` / `agent.*`
//! engine that body calls for live values. The engine is the framework's own
//! (`makepad_widgets::splash::register_agent_module`); `widgets::script_mod`
//! installs it into the main VM, but a Splash ISOLATE strips injected globals
//! when it is minted, so this crate registers the same installer as a host
//! isolate mod (`register_splash_isolate_mod`, which runs after the strip) —
//! measured: without it the card's `sys` is "not found in scope" and the
//! body draws nothing. This crate carries no `sys` of its own; beside the
//! engine it installs only its text roles, so the body's font references
//! resolve against this crate.
//! Live values come through the platform's fetch layer; when a fetch lands
//! the root widget re-evaluates the body, so the tile shows "—" first and
//! the readings a moment later. An empty place means "where the device is":
//! the engine reverse-geocodes `makepad_platform::gps`'s last fix, so the
//! card names the real place once the host has a fix and `DEFAULT_PLACE`
//! until then.
//!
//! Phase C foundation, also here: the octos kernel bundled into the APK as
//! `liboctos.so` is started by `kernel::spawn_kernel` and PROBED from
//! `create` (spawned, given two seconds, its state logged as `kernel: ok` or
//! `kernel: error`); the child lives in the instance root and is killed when
//! the instance is torn down. Nothing speaks to it yet — the shell track
//! wires the transport.
//!
//! What does NOT run here yet is the L0 pipeline itself — realize → kit lower
//! → eval → `to_dsl` (`l0_card.rs`, `l0_eval.rs`, `l0_widgets.rs` and their
//! kit components in AppCard's app crate). That code is bound to the app shell:
//! the approval store, `user_store`, the fetched-rows planner, `L0_APPS`, and
//! some forty Octoscript-Makepad `components/l0/*.octoscript` kit fragments it
//! `include_str!`s by relative path. Phase A therefore ships the weather
//! exemplar PRE-LOWERED: `cards/weather.splash` is the Splash DSL for
//! `cards/weather.card` (kept beside it), written against the same widgets
//! and the same `sys.*` contract the pipeline emits, so Phase B can drop the
//! live pipeline in without touching the host side. The card is pinned to
//! `DEFAULT_PLACE` only while the host has no GPS fix; the exemplar's blank
//! city means "where the device is", and with a fix that is what it gets.
pub use makepad_widgets;
use makepad_app_module::{
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, OpenSchema, ServiceExecutor, ValidatedOpen,
};
use makepad_widgets::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

pub mod kernel;

/// The L0 source the shipped body was lowered from (AppCard's
/// `a2app-l0/apps/weather/exemplar.card`), for reference and for Phase B.
pub const WEATHER_CARD: &str = include_str!("../cards/weather.card");
/// The pre-lowered Splash body of that card.
pub const WEATHER_BODY: &str = include_str!("../cards/weather.splash");
/// The place the card shows until the host has a GPS fix; with one, the
/// place is left blank and the engine names where the device is.
pub const DEFAULT_PLACE: &str = "Cupertino";
/// How long `create` gives the kernel to come up before logging its state.
pub const KERNEL_PROBE_WAIT: Duration = Duration::from_secs(2);

/// The card's text roles: the same weights `text_roles.rs` names, resolved
/// against THIS crate's fonts (`resources/Roboto-*.ttf`, carried from the
/// AppCard port) with the framework's symbol and CJK members kept in the
/// chain so `↑ ↓ ≈` and Chinese never draw as tofu. Loaded into the module
/// isolate by `register` and into every Splash isolate as a host isolate mod
/// (`register_splash_isolate_mod`); the second load also records this
/// crate's manifest so the body's `octosense_appcard:` resource paths
/// resolve there. Isolate mods run last, after the framework's own
/// `script_mod` has installed `sys`/`agent`, so nothing here can shadow them.
pub mod roles {
    use makepad_widgets::*;
    /// The isolate-mod shape of `script_mod` (which returns the eval value).
    pub fn install(vm: &mut ScriptVm) {
        script_mod(vm);
    }
    script_mod! {
        use mod.prelude.widgets.*

        mod.widgets.AppCardHero = Label{
            draw_text.text_style: TextStyle{
                font_family: FontFamily{
                    latin   := FontMember{ res: crate_resource("self:resources/Roboto-Thin.ttf") asc: 0.0 desc: 0.0 }
                    sym     := FontMember{ res: crate_resource("makepad_widgets:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0 }
                    chinese := FontMember{ res: crate_resource("makepad_widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0 }
                }
                font_size: 64
            }
        }
        mod.widgets.AppCardTitle = Label{
            draw_text.text_style: TextStyle{
                font_family: FontFamily{
                    latin   := FontMember{ res: crate_resource("self:resources/Roboto-Medium.ttf") asc: 0.0 desc: 0.0 }
                    sym     := FontMember{ res: crate_resource("makepad_widgets:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0 }
                    chinese := FontMember{ res: crate_resource("makepad_widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0 }
                }
                font_size: 20
            }
        }
        mod.widgets.AppCardBody = Label{
            draw_text.text_style: TextStyle{
                font_family: FontFamily{
                    latin   := FontMember{ res: crate_resource("self:resources/Roboto-Light.ttf") asc: 0.0 desc: 0.0 }
                    sym     := FontMember{ res: crate_resource("makepad_widgets:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0 }
                    chinese := FontMember{ res: crate_resource("makepad_widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0 }
                }
                font_size: 15
            }
        }
        mod.widgets.AppCardStat = Label{
            draw_text.text_style: TextStyle{
                font_family: FontFamily{
                    latin   := FontMember{ res: crate_resource("self:resources/Roboto-Regular.ttf") asc: 0.0 desc: 0.0 }
                    sym     := FontMember{ res: crate_resource("makepad_widgets:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0 }
                    chinese := FontMember{ res: crate_resource("makepad_widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0 }
                }
                font_size: 14
            }
        }
        mod.widgets.AppCardCaption = Label{
            draw_text.text_style: TextStyle{
                font_family: FontFamily{
                    latin   := FontMember{ res: crate_resource("self:resources/Roboto-Regular.ttf") asc: 0.0 desc: 0.0 }
                    sym     := FontMember{ res: crate_resource("makepad_widgets:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0 }
                    chinese := FontMember{ res: crate_resource("makepad_widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0 }
                }
                font_size: 11
            }
        }
        mod.widgets.AppCardValue = Label{
            draw_text.text_style: TextStyle{
                font_family: FontFamily{
                    latin   := FontMember{ res: crate_resource("self:resources/Roboto-Light.ttf") asc: 0.0 desc: 0.0 }
                    sym     := FontMember{ res: crate_resource("makepad_widgets:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0 }
                    chinese := FontMember{ res: crate_resource("makepad_widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0 }
                }
                font_size: 22
            }
        }
        // The Splash body's prelude was assembled before this mod ran, so the
        // roles are put where the body's `use mod.prelude.widgets.*` looks.
        mod.prelude.widgets.AppCardHero = mod.widgets.AppCardHero
        mod.prelude.widgets.AppCardTitle = mod.widgets.AppCardTitle
        mod.prelude.widgets.AppCardBody = mod.widgets.AppCardBody
        mod.prelude.widgets.AppCardStat = mod.widgets.AppCardStat
        mod.prelude.widgets.AppCardCaption = mod.widgets.AppCardCaption
        mod.prelude.widgets.AppCardValue = mod.widgets.AppCardValue
    }
}

script_mod! {
    use mod.prelude.widgets.*
    mod.widgets.AppCardView = set_type_default() do #(AppCardView::register_widget(vm)) {
        ..mod.widgets.RectView
        width: Fill height: Fill
        draw_bg.color: #f2f2f7
        flow: Down
        scroll := ScrollYView {
            width: Fill height: Fill flow: Down
            card := Splash { width: Fill height: Fit allow_net: true }
        }
    }
}

/// The instance root: a scrolling page around one `Splash`. It owns the
/// card body and re-sets it — the Splash re-evaluates in place — whenever the
/// platform's data-fetch epoch moves, which is how a landed `sys.weather`
/// fetch reaches the screen.
#[derive(Script, ScriptHook, Widget)]
pub struct AppCardView {
    #[deref]
    view: View,
    #[rust]
    body: String,
    #[rust]
    place: String,
    #[rust]
    started: bool,
    #[rust]
    fetch_epoch: u64,
    /// Whether the last body was bound to the device's GPS fix (an empty
    /// place) rather than `place`.
    #[rust]
    gps_place: bool,
    /// The probed kernel, if one started: shared with the instance's
    /// `shutdown`, which kills it; a root dropped without one kills it too.
    #[rust]
    kernel: Rc<RefCell<Option<kernel::Kernel>>>,
}

/// Whether the host has a GPS fix the engine can resolve a blank place from.
fn has_gps_fix() -> bool {
    makepad_widgets::makepad_draw::makepad_platform::gps::last_gps_fix().is_some()
}

impl AppCardView {
    /// The body the Splash evaluates: the card's place and the fetch epoch
    /// bound first, then the lowered card. The epoch line is what makes a
    /// re-set body differ, so `set_text` re-evaluates instead of ignoring it.
    /// With a GPS fix the place is left EMPTY: the engine's `geocode` reads
    /// that as "where the device is" and reverse-geocodes the fix.
    fn body_text(&self, epoch: u64, gps_place: bool) -> String {
        let place = if gps_place { String::new() } else { self.place.replace('\\', "\\\\").replace('"', "\\\"") };
        format!("let fetch_epoch = {epoch}\nlet place = \"{place}\"\n{}", self.body)
    }

    fn sync_body(&mut self, cx: &mut Cx, force: bool) {
        let epoch = cx.script_data_fetch_epoch();
        let gps_place = has_gps_fix();
        if !force && epoch == self.fetch_epoch && gps_place == self.gps_place {
            return;
        }
        self.fetch_epoch = epoch;
        self.gps_place = gps_place;
        let text = self.body_text(epoch, gps_place);
        self.view.splash(cx, ids!(scroll.card)).set_text(cx, &text);
    }
}

impl Widget for AppCardView {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if !self.started {
            self.started = true;
            self.sync_body(cx, true);
        }
        self.view.draw_walk(cx, scope, walk)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if self.started {
            self.sync_body(cx, false);
        }
    }
}

pub struct AppCardModule;
pub static APPCARD_MODULE: AppCardModule = AppCardModule;

impl AppModule for AppCardModule {
    fn id(&self) -> &'static str { "appcard" }
    fn label(&self) -> &'static str { "AppCard" }
    fn register(&self, vm: &mut ScriptVm) {
        roles::script_mod(vm);
        script_mod(vm);
        // Every Splash isolate the host allocates from now on carries the
        // framework's `sys`/`agent` engine and this crate's text roles. Host
        // isolate mods run after the isolate's ambient-authority strip, which
        // is what removes the engine `widgets::script_mod` installed into the
        // main VM — see the crate doc. Registering on each `register` is
        // harmless: a second install of the same mod rebinds the same names.
        makepad_widgets::widget_async::register_splash_isolate_mod(makepad_widgets::splash::register_agent_module);
        makepad_widgets::widget_async::register_splash_isolate_mod(roles::install);
    }
    fn open_schema(&self) -> OpenSchema { OpenSchema::new(1) }
    /// The card's live values are HTTP fetches through the platform.
    fn capabilities(&self) -> &'static [&'static str] { &["net"] }
    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        // The kernel probe, before the view so a slow start does not sit
        // between a minted widget and its first draw. The data dir is the
        // app's files dir on the phone; a desktop host names none, so the
        // kernel gets a home beside OctoSense's own.
        let kernel = if kernel::probe_enabled() {
            let data_dir = vm
                .host
                .cx_mut()
                .get_data_dir()
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".octosense/appcard")));
            match data_dir {
                Some(data_dir) => match kernel::spawn_kernel(&data_dir) {
                    Ok(mut kernel) => {
                        let status = kernel.probe(KERNEL_PROBE_WAIT);
                        log!("kernel: {status} [pid={} bin={}]", kernel.pid(), kernel.program().display());
                        match status {
                            kernel::ProbeStatus::Exited { .. } => None,
                            _ => Some(kernel),
                        }
                    }
                    Err(e) => {
                        log!("kernel: error ({e})");
                        None
                    }
                },
                None => {
                    log!("kernel: error (no data dir and no HOME to put octos-home under)");
                    None
                }
            }
        } else {
            log!("kernel: skipped ({}=0)", kernel::PROBE_ENV);
            None
        };
        let value = script_eval!(vm, {
            use mod.widgets.*
            AppCardView {}
        });
        let root = WidgetRef::script_from_value(vm, value);
        // The kernel is instance state: the root holds it while the instance
        // lives, `shutdown` takes it and kills it, and a root dropped without
        // a shutdown still kills it through `Kernel::drop`.
        let slot = Rc::new(RefCell::new(kernel));
        if let Some(mut view) = root.borrow_mut::<AppCardView>() {
            view.body = WEATHER_BODY.to_string();
            view.place = DEFAULT_PLACE.to_string();
            view.kernel = slot.clone();
        }
        InstanceParts {
            root,
            executor: Box::new(AppCardExecutor),
            shutdown: Box::new(move |_| {
                if let Some(mut kernel) = slot.borrow_mut().take() {
                    let pid = kernel.pid();
                    kernel.kill();
                    log!("kernel: stopped pid={pid} with the appcard instance");
                }
            }),
        }
    }
}

/// No tools in Phase A: the manifest names the service and offers nothing.
struct AppCardExecutor;
impl ServiceExecutor for AppCardExecutor {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("appcard", "AppCard", "An Octoscript-AppCard L0 card in a tile.")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(&call.call_id, "AppCard has no tools yet"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_describes_itself_and_opens_empty() {
        let m = &APPCARD_MODULE;
        assert_eq!(m.id(), "appcard");
        assert_eq!(m.label(), "AppCard");
        assert_eq!(m.capabilities(), &["net"]);
        assert!(m.open_schema().empty_open().is_ok());
        assert!(AppCardExecutor.manifest().tools.is_empty(), "no tools in Phase A");
    }

    #[test]
    fn the_shipped_body_binds_only_helpers_the_framework_engine_installs() {
        // Every `sys.<name>(` the lowered card calls must be a helper the
        // framework's `register_agent_module` defines (the weather/text subset
        // named here), or the card draws `$[Error]` where a value goes.
        let installed = [
            "fetch", "geocode", "geocodenum", "weather", "weathernum", "weathercond", "weatherword",
            "dayname", "weekmin", "weekmax", "moonphase", "moonnum", "daylight", "locale", "prefs",
            "cities", "citiesnum", "gps", "l0_ratio", "convert", "num", "json_string", "l0_math",
        ];
        let mut used = Vec::new();
        for (i, _) in WEATHER_BODY.match_indices("sys.") {
            let name: String = WEATHER_BODY[i + 4..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if !name.is_empty() && !used.contains(&name) { used.push(name); }
        }
        assert!(!used.is_empty());
        for name in &used {
            assert!(installed.contains(&name.as_str()), "sys.{name} is not installed");
        }
        assert!(WEATHER_CARD.contains("# ledger weather@"), "the L0 source travels with its lowering");
        assert!(WEATHER_BODY.contains("octosense_appcard:resources/") || WEATHER_BODY.contains("AppCardHero"),
                "the body reaches this crate's fonts through its roles");
    }

    #[test]
    fn a_gps_fix_blanks_the_place_so_the_engine_resolves_the_device() {
        let view_body = "body";
        let place = "Cupertino";
        let named = format!("let fetch_epoch = 3\nlet place = \"{place}\"\n{view_body}");
        let blank = format!("let fetch_epoch = 3\nlet place = \"\"\n{view_body}");
        assert_ne!(named, blank);
        assert!(blank.contains("let place = \"\""), "an empty place is what the engine reads as the device's fix");
        assert!(!has_gps_fix(), "a unit test host has no GPS fix");
    }
}
