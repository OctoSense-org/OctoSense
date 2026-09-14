//! Octoscript-AppCard inside OctoSense, Phase A: one L0 card in a home tile,
//! in-process, in an isolate of its own.
//!
//! What runs here is the RENDER half of AppCard: the `appcard` widget set on
//! the makepad fork (kit components, text roles, WeatherIcon, TempBar, SunArc,
//! MoonPhase, AqiContour, matplot), a `Splash` whose body is the lowered card,
//! and the `sys.*` helpers that body calls for live values (`sys/`), installed
//! into every Splash isolate through `register_splash_isolate_mod`. Live
//! values come through the platform's fetch layer; when a fetch lands the
//! root widget re-evaluates the body, so the tile shows "—" first and the
//! readings a moment later.
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
//! live pipeline in without touching the host side. The card is pinned to one
//! place (`DEFAULT_PLACE`) because the exemplar's blank city means "where the
//! device is" and this host has no location yet.
pub use makepad_widgets;
use makepad_app_module::{
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, OpenSchema, ServiceExecutor, ValidatedOpen,
};
use makepad_widgets::*;

pub mod sys;

/// The L0 source the shipped body was lowered from (AppCard's
/// `a2app-l0/apps/weather/exemplar.card`), for reference and for Phase B.
pub const WEATHER_CARD: &str = include_str!("../cards/weather.card");
/// The pre-lowered Splash body of that card.
pub const WEATHER_BODY: &str = include_str!("../cards/weather.splash");
/// The place the card shows until the host can answer `sys.gps`.
pub const DEFAULT_PLACE: &str = "Cupertino";

/// The card's text roles: the same weights `text_roles.rs` names, resolved
/// against THIS crate's fonts (`resources/Roboto-*.ttf`, carried from the
/// AppCard port) with the framework's symbol and CJK members kept in the
/// chain so `↑ ↓ ≈` and Chinese never draw as tofu. Loaded into the module
/// isolate by `register` and into every Splash isolate by the `sys`
/// installer; the second load also records this crate's manifest so the
/// body's `octosense_appcard:` resource paths resolve there.
pub mod roles {
    use makepad_widgets::*;
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
}

impl AppCardView {
    /// The body the Splash evaluates: the card's place and the fetch epoch
    /// bound first, then the lowered card. The epoch line is what makes a
    /// re-set body differ, so `set_text` re-evaluates instead of ignoring it.
    fn body_text(&self, epoch: u64) -> String {
        let place = self.place.replace('\\', "\\\\").replace('"', "\\\"");
        format!("let fetch_epoch = {epoch}\nlet place = \"{place}\"\n{}", self.body)
    }

    fn sync_body(&mut self, cx: &mut Cx, force: bool) {
        let epoch = cx.script_data_fetch_epoch();
        if !force && epoch == self.fetch_epoch {
            return;
        }
        self.fetch_epoch = epoch;
        let text = self.body_text(epoch);
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
        // Every Splash isolate the host allocates from now on carries `sys`.
        // Registering on each `register` is harmless: a second install of the
        // same mod rebinds the same names.
        makepad_widgets::widget_async::register_splash_isolate_mod(sys::install);
    }
    fn open_schema(&self) -> OpenSchema { OpenSchema::new(1) }
    /// The card's live values are HTTP fetches through the platform.
    fn capabilities(&self) -> &'static [&'static str] { &["net"] }
    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, {
            use mod.widgets.*
            AppCardView {}
        });
        let root = WidgetRef::script_from_value(vm, value);
        if let Some(mut view) = root.borrow_mut::<AppCardView>() {
            view.body = WEATHER_BODY.to_string();
            view.place = DEFAULT_PLACE.to_string();
        }
        InstanceParts {
            root,
            executor: Box::new(AppCardExecutor),
            shutdown: Box::new(|_| {}),
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
    fn the_shipped_body_binds_only_helpers_the_sys_module_installs() {
        // Every `sys.<name>(` the lowered card calls must be a helper the
        // installer defines, or the card draws `$[Error]` where a value goes.
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
}
