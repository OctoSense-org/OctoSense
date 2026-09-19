//! The maps surface, after Google Maps: one `MapView` that is never
//! rebuilt, full-bleed, and over it a layer per screen that the model shows
//! or hides — Explore's search pill and round buttons here; Search, Place,
//! Directions and Navigation join it. Everything is drawn in the app's skin
//! (`Skin`): Google Maps' light look, or its dark counterpart when the host
//! is dark. The location updates, the timers and the map's camera are the
//! view's. `ensure_started` seeds everything on the first event or draw,
//! whichever the host gives it first: a window has a `Startup` event, a
//! module instance does not.

use crate::geo::LonLat;
use crate::model::{
    LocationAsk, LocationFix, LocationState, MapsModel, Screen, Skin, LOCATION_FIX_TIMEOUT_SECONDS,
};
use crate::HOSTED_TILES;
use makepad_widgets::*;

/// How long a status line (`Locating…`, an error) stays up.
const STATUS_SECONDS: f64 = 4.0;
/// The zoom the locate button flies to: streets and their names.
const LOCATE_ZOOM: f64 = 15.5;
/// Rotation or tilt, in degrees, under which the map counts as north-up
/// and flat: the widget's own snap leaves a fraction of a degree.
const LEVEL_DEGREES: f64 = 1.0;
/// `MapView::set_theme`: its light and its night style.
const MAP_THEME_LIGHT: u32 = 0;
const MAP_THEME_NIGHT: u32 = 1;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // The skin's roles, resolved when this module is evaluated: the host
    // re-runs it on a light/dark change and re-applies the root.
    let c_card = #(Skin::for_vm(vm).card)
    let c_ink = #(Skin::for_vm(vm).ink)
    let c_secondary = #(Skin::for_vm(vm).secondary)
    let c_hairline = #(Skin::for_vm(vm).hairline)
    let c_field = #(Skin::for_vm(vm).field)
    let c_accent = #(Skin::for_vm(vm).accent)
    let c_clear = #00000000

    // Text without `Label`'s own padding, so rows are as tall as their text.
    let Text = Label{padding: 0 draw_text +: {color: c_ink text_style: theme.font_regular{font_size: 14}}}
    let Caption = Text{draw_text +: {color: c_secondary text_style: theme.font_regular{font_size: 12}}}
    let Hairline = SolidView{width: Fill height: 0.5 draw_bg.color: c_hairline}

    // A button with no face of its own: an icon, or a line of text.
    let Plain = ButtonFlat{padding: 0 margin: 0 text: "" spacing: 0 align: Center
        draw_bg +: {
            border_size: uniform(0.0)
            color: uniform(c_clear) color_hover: uniform(c_clear) color_down: uniform(#00000018) color_focus: uniform(c_clear)
            border_color: uniform(c_clear) border_color_hover: uniform(c_clear) border_color_down: uniform(c_clear) border_color_focus: uniform(c_clear)
        }
    }
    // A round button floating over the map: a disc in the card colour with
    // a hairline rim, its icon in the ink until `render` tints it.
    let Fab = Plain{width: 48 height: 48 icon_walk: Walk{width: 20 height: 20}
        draw_bg +: {
            border_size: uniform(1.0) border_radius: uniform(24.0)
            color: uniform(c_card) color_hover: uniform(c_card) color_down: uniform(c_field) color_focus: uniform(c_card)
            border_color: uniform(c_hairline) border_color_hover: uniform(c_hairline) border_color_down: uniform(c_hairline) border_color_focus: uniform(c_hairline)
        }
        draw_icon +: {color: c_ink}
    }
    // Anything that floats over the map on a card: the search pill, the
    // status line, the sheets.
    let Floating = RoundedShadowView{width: Fill height: Fit
        draw_bg +: {color: c_card border_radius: uniform(24.0) shadow_color: #00000040 shadow_radius: uniform(8.0) shadow_offset: uniform(vec2(0.0, 2.0))}
    }
    // A switch of the layers sheet: a pill in the accent when on.
    let Switch = Toggle{text: "" margin: 0 padding: 0 width: 42 height: 26
        draw_bg +: {
            size: uniform(26.0)
            border_size: uniform(0.0)
            color: uniform(c_field) color_hover: uniform(c_field) color_down: uniform(c_field) color_focus: uniform(c_field)
            color_active: uniform(c_accent) color_disabled: uniform(c_field)
            border_color: uniform(c_clear) border_color_hover: uniform(c_clear) border_color_active: uniform(c_clear) border_color_focus: uniform(c_clear)
            mark_color: uniform(#ffffff) mark_color_hover: uniform(#ffffff) mark_color_down: uniform(#ffffff)
            mark_color_active: uniform(#ffffff) mark_color_active_hover: uniform(#ffffff)
        }
    }
    // A row of the layers sheet: its label fills the width, so the switch
    // after it sits at the right.
    let SwitchRow = View{width: Fill height: 52 flow: Right align: Align{y: 0.5} padding: Inset{left: 20 right: 16}}
    let SwitchLabel = Text{width: Fill draw_text.text_style: theme.font_regular{font_size: 16}}

    mod.widgets.MapsViewBase = #(MapsView::register_widget(vm))
    mod.widgets.MapsView = set_type_default() do mod.widgets.MapsViewBase{
        width: Fill height: Fill flow: Overlay

        // The one map, under everything, for as long as the app lives.
        // `debug_cam`: the camera readout is a workbench tool.
        map := MapView{width: Fill height: Fill min_zoom: 3.0 max_zoom: 20.0 buildings_3d: true debug_cam: false}

        // Explore: no ground of its own, so the map beneath keeps its touches.
        explore := View{width: Fill height: Fill flow: Down padding: Inset{left: 12 right: 12 top: 10 bottom: 14}
            pill := Floating{height: 48 flow: Right spacing: 12 align: Align{y: 0.5} padding: Inset{left: 16 right: 16} cursor: MouseCursor.Hand
                Icon{icon_walk: Walk{width: 18 height: 18} draw_icon +: {svg: crate_resource("self:resources/icons/search.svg") color: c_secondary}}
                pill_text := Text{width: Fill max_lines: 1 text_overflow: Ellipsis text: "Search here" draw_text +: {color: c_secondary text_style: theme.font_regular{font_size: 16}}}
            }
            View{width: Fill height: Fit flow: Right padding: Inset{top: 12}
                View{width: Fill height: Fit}
                layers := Fab{width: 44 height: 44 draw_bg +: {border_radius: uniform(22.0)} draw_icon.svg: crate_resource("self:resources/icons/layers.svg")}
            }
            // What the locate button has to say, under the pill.
            View{width: Fill height: Fit align: Align{x: 0.5} padding: Inset{top: 8}
                status := Floating{visible: false width: Fit padding: Inset{left: 16 right: 16 top: 9 bottom: 9}
                    draw_bg +: {border_radius: uniform(18.0)}
                    status_text := Text{draw_text.text_style: theme.font_regular{font_size: 13}}
                }
            }
            View{width: Fill height: Fill}
            View{width: Fill height: Fit flow: Right align: Align{y: 1.0}
                // The licence's line, where the data's readers look for it.
                View{width: Fill height: Fit
                    RoundedView{width: Fit height: Fit padding: Inset{left: 6 right: 6 top: 3 bottom: 3}
                        draw_bg +: {color: c_card border_radius: uniform(6.0)}
                        attribution := Caption{text: "© OpenStreetMap contributors" draw_text.text_style: theme.font_regular{font_size: 10}}
                    }
                }
                View{width: Fit height: Fit flow: Down spacing: 12
                    compass := Fab{visible: false draw_icon +: {svg: crate_resource("self:resources/icons/compass.svg") color: c_accent}}
                    locate := Fab{draw_icon.svg: crate_resource("self:resources/icons/locate.svg")}
                    locate_active := Fab{visible: false draw_icon +: {svg: crate_resource("self:resources/icons/locate-active.svg") color: c_accent}}
                }
            }
        }

        // The layers sheet: a scrim and a card at the bottom.
        layers_layer := View{visible: false width: Fill height: Fill flow: Overlay
            // A `SolidView`: a plain view's ground has no shader at this
            // revision and would draw nothing.
            scrim := SolidView{width: Fill height: Fill draw_bg.color: #00000055 cursor: MouseCursor.Default}
            View{width: Fill height: Fill flow: Down align: Align{y: 1.0} padding: Inset{left: 10 right: 10 bottom: 12}
                layers_card := Floating{flow: Down padding: Inset{top: 8 bottom: 8}
                    View{width: Fill height: 48 flow: Right align: Align{y: 0.5} padding: Inset{left: 20 right: 8}
                        Text{width: Fill text: "Map details" draw_text.text_style: theme.font_bold{font_size: 17}}
                        layers_close := Plain{width: 40 height: 40 icon_walk: Walk{width: 14 height: 14}
                            draw_icon +: {svg: crate_resource("self:resources/icons/close.svg") color: c_secondary}}
                    }
                    Hairline{}
                    SwitchRow{SwitchLabel{text: "Dark map"} dark_map := Switch{}}
                    SwitchRow{SwitchLabel{text: "3D buildings"} buildings := Switch{}}
                    SwitchRow{SwitchLabel{text: "Labels"} labels := Switch{}}
                    SwitchRow{SwitchLabel{text: "Distances in miles"} miles := Switch{}}
                }
            }
        }
    }
}

/// The map, the screens over it and what drives them: one widget that owns
/// everything the standalone window's `App` would otherwise own, so a module
/// host gets the same maps a window does.
#[derive(Script, ScriptHook, Widget)]
pub struct MapsView {
    #[deref]
    view: View,
    /// Set once, on the first event or draw.
    #[rust]
    started: bool,
    #[rust]
    model: MapsModel,
    #[rust]
    location: LocationState,
    /// Runs while the locate button waits for its first fix.
    #[rust]
    location_timeout: Option<Timer>,
    /// Takes the status line down again.
    #[rust]
    status_timer: Option<Timer>,
    #[rust]
    layers_open: bool,
    /// What `render` last showed, so it touches widgets only on a change.
    #[rust]
    shown: Option<Shown>,
    /// The map theme last applied, so a skin change re-applies it.
    #[rust]
    map_theme: Option<u32>,
}

/// What of the state the widgets show: `render` compares it whole.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shown {
    screen: Screen,
    layers_open: bool,
    location: LocationState,
    level: bool,
}

impl MapsView {
    pub fn model(&self) -> &MapsModel {
        &self.model
    }

    pub fn location(&self) -> LocationState {
        self.location
    }

    #[cfg(test)]
    pub(crate) fn layers_open(&self) -> bool {
        self.layers_open
    }

    #[cfg(test)]
    pub(crate) fn status_text(&self, cx: &Cx) -> Option<String> {
        self.view
            .widget(cx, ids!(status))
            .visible()
            .then(|| self.view.label(cx, ids!(status_text)).text())
    }

    fn map(&self, cx: &Cx) -> MapViewRef {
        self.view.map_view(cx, ids!(map))
    }

    /// The timers stop and the platform's location updates with them;
    /// nothing else this widget owns outlives its isolate.
    pub fn shutdown(&mut self, cx: &mut Cx) {
        for timer in [self.location_timeout.take(), self.status_timer.take()]
            .into_iter()
            .flatten()
        {
            cx.stop_timer(timer);
        }
        if self.location.failed() {
            cx.stop_location_updates();
        }
    }

    /// Start on the first event or draw, whichever comes first.
    fn ensure_started(&mut self, cx: &mut Cx) {
        if self.started {
            return;
        }
        self.started = true;
        let map = self.map(cx);
        map.set_source_config(cx, TileSourceConfig::http_archive(HOSTED_TILES));
        self.apply_settings(cx);
        let settings = &self.model.settings;
        map.set_center(cx, settings.center.lon, settings.center.lat);
        map.set_map_zoom(cx, settings.zoom);
        self.render(cx);
    }

    /// Open the layers sheet, as its button does.
    pub fn open_layers(&mut self, cx: &mut Cx) {
        self.layers_open = true;
        self.render(cx);
    }

    /// The camera the app was given to open on (`--at`), before it starts.
    pub fn set_initial_camera(&mut self, center: LonLat, zoom: f64) {
        self.model.settings.center = center;
        self.model.settings.zoom = zoom;
    }

    /// The layer switches as the map's own state.
    fn apply_settings(&mut self, cx: &mut Cx) {
        let map = self.map(cx);
        let settings = &self.model.settings;
        map.set_buildings_3d(cx, settings.buildings_3d);
        map.set_labels_visible(cx, settings.labels);
        self.apply_map_theme(cx);
    }

    /// The map is dark when the person said so, else when the skin is.
    fn apply_map_theme(&mut self, cx: &mut Cx) {
        let dark = self
            .model
            .settings
            .dark_map
            .unwrap_or(!Skin::current().light);
        let theme = if dark {
            MAP_THEME_NIGHT
        } else {
            MAP_THEME_LIGHT
        };
        if self.map_theme != Some(theme) {
            self.map_theme = Some(theme);
            self.map(cx).set_theme(cx, theme);
        }
    }

    // ---- Location ----

    /// The locate button, or a screen that wants a fix.
    fn ask_location(&mut self, cx: &mut Cx) {
        match self.location.asked() {
            LocationAsk::Start => {
                self.show_status(cx, "Locating…");
                cx.start_location_updates();
                self.location_timeout = Some(cx.start_timeout(LOCATION_FIX_TIMEOUT_SECONDS));
            }
            LocationAsk::Recenter => {
                if let Some(fix) = self.model.fix() {
                    self.map(cx).fly_to(cx, fix.lon, fix.lat, LOCATE_ZOOM);
                }
            }
            LocationAsk::Ignore => {}
        }
        self.render(cx);
    }

    fn on_location_update(&mut self, cx: &mut Cx, fix: &LocationUpdateEvent) {
        let kind = self.location.received_fix();
        if kind == LocationFix::Ignore {
            return;
        }
        self.model.set_fix(LonLat::new(fix.lon, fix.lat));
        let map = self.map(cx);
        map.set_puck(
            cx,
            Some(MapPuck::new(
                fix.lon,
                fix.lat,
                fix.heading_deg,
                fix.accuracy_m,
            )),
        );
        if kind == LocationFix::First {
            self.stop_location_timeout(cx);
            self.hide_status(cx);
            map.fly_to(cx, fix.lon, fix.lat, LOCATE_ZOOM);
            self.render(cx);
        }
    }

    /// An error, or the timeout: say why, once, and let the next tap retry.
    fn fail_location(&mut self, cx: &mut Cx, why: &str) {
        if !self.location.failed() {
            return;
        }
        self.stop_location_timeout(cx);
        cx.stop_location_updates();
        self.show_status(cx, why);
        self.render(cx);
    }

    fn stop_location_timeout(&mut self, cx: &mut Cx) {
        if let Some(timer) = self.location_timeout.take() {
            cx.stop_timer(timer);
        }
    }

    // ---- The status line ----

    fn show_status(&mut self, cx: &mut Cx, text: &str) {
        self.view.label(cx, ids!(status_text)).set_text(cx, text);
        self.view.widget(cx, ids!(status)).set_visible(cx, true);
        if let Some(timer) = self.status_timer.take() {
            cx.stop_timer(timer);
        }
        self.status_timer = Some(cx.start_timeout(STATUS_SECONDS));
        self.view.redraw(cx);
    }

    fn hide_status(&mut self, cx: &mut Cx) {
        if let Some(timer) = self.status_timer.take() {
            cx.stop_timer(timer);
        }
        self.view.widget(cx, ids!(status)).set_visible(cx, false);
        self.view.redraw(cx);
    }

    // ---- The map's camera ----

    /// Whether the map is north-up and flat, give or take the snap.
    fn is_level(&self, cx: &Cx) -> bool {
        let map = self.map(cx);
        let rotation = map.rotation().rem_euclid(360.0);
        rotation.min(360.0 - rotation) < LEVEL_DEGREES && map.tilt().abs() < LEVEL_DEGREES
    }

    /// The compass: north-up and flat again.
    fn level_map(&mut self, cx: &mut Cx) {
        let map = self.map(cx);
        map.set_rotation(cx, 0.0);
        map.set_tilt(cx, 0.0);
        self.render(cx);
    }

    // ---- Back ----

    /// One step back: the layers sheet, then the model's screens. `false`
    /// when nothing was left, so the host handles it.
    pub fn back(&mut self, cx: &mut Cx) -> bool {
        let handled = if self.layers_open {
            self.layers_open = false;
            true
        } else {
            self.model.back()
        };
        if handled {
            self.render(cx);
        }
        handled
    }

    // ---- Showing the state ----

    /// Show what changed: which layers are up, the locate button's face,
    /// the compass.
    fn render(&mut self, cx: &mut Cx) {
        let shown = Shown {
            screen: self.model.screen(),
            layers_open: self.layers_open,
            location: self.location,
            level: self.is_level(cx),
        };
        if self.shown == Some(shown) {
            return;
        }
        self.shown = Some(shown);
        self.view
            .widget(cx, ids!(explore))
            .set_visible(cx, shown.screen == Screen::Explore);
        self.view
            .widget(cx, ids!(layers_layer))
            .set_visible(cx, shown.layers_open);
        let following = shown.location == LocationState::Active;
        self.view
            .widget(cx, ids!(locate))
            .set_visible(cx, !following);
        self.view
            .widget(cx, ids!(locate_active))
            .set_visible(cx, following);
        self.view
            .widget(cx, ids!(compass))
            .set_visible(cx, !shown.level);
        self.view.redraw(cx);
    }

    /// The layers sheet's switches, as the settings stand.
    fn sync_switches(&mut self, cx: &mut Cx) {
        let settings = &self.model.settings;
        let dark = settings.dark_map.unwrap_or(!Skin::current().light);
        let miles = self.model.units() == crate::geo::Units::Imperial;
        let states = [
            (ids!(dark_map), dark),
            (ids!(buildings), settings.buildings_3d),
            (ids!(labels), settings.labels),
            (ids!(miles), miles),
        ];
        for (id, on) in states {
            let switch = self.view.check_box(cx, id);
            if switch.active(cx) != on {
                // A cut, not a play: the sheet opens showing what is.
                switch.set_active(cx, on, Animate::No);
                self.view.redraw(cx);
            }
        }
    }

    fn handle_layers_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let layer = self.view.widget(cx, ids!(layers_layer));
        if self.view.button(cx, ids!(layers_close)).clicked(actions)
            || tapped(cx, &layer, ids!(scrim), actions)
        {
            self.layers_open = false;
            self.render(cx);
            return;
        }
        let mut changed = false;
        if let Some(on) = self.view.check_box(cx, ids!(dark_map)).changed(actions) {
            self.model.settings.dark_map = Some(on);
            changed = true;
        }
        if let Some(on) = self.view.check_box(cx, ids!(buildings)).changed(actions) {
            self.model.settings.buildings_3d = on;
            changed = true;
        }
        if let Some(on) = self.view.check_box(cx, ids!(labels)).changed(actions) {
            self.model.settings.labels = on;
            changed = true;
        }
        if let Some(on) = self.view.check_box(cx, ids!(miles)).changed(actions) {
            self.model.settings.units = Some(if on {
                crate::geo::Units::Imperial
            } else {
                crate::geo::Units::Metric
            });
            changed = true;
        }
        if changed {
            self.apply_settings(cx);
        }
    }
}

/// A tap (not the end of a drag) on the child `id` of `parent`.
fn tapped(cx: &Cx, parent: &WidgetRef, id: &[LiveId], actions: &Actions) -> bool {
    parent
        .widget(cx, id)
        .as_view()
        .finger_up(actions)
        .is_some_and(|up| up.was_tap())
}

impl Widget for MapsView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.ensure_started(cx);
        match event {
            Event::BackPressed { handled } => {
                if !handled.get() && self.back(cx) {
                    handled.set(true);
                }
            }
            Event::LocationUpdate(fix) => self.on_location_update(cx, fix),
            Event::LocationError(error) => {
                let why = match error {
                    LocationErrorEvent::PermissionDenied => "Location is off for OctosMap",
                    LocationErrorEvent::Unavailable(_) => "Could not find your location",
                };
                self.fail_location(cx, why);
            }
            _ => {}
        }
        if self
            .location_timeout
            .as_ref()
            .is_some_and(|t| t.is_event(event).is_some())
        {
            self.location_timeout = None;
            self.fail_location(cx, "Could not find your location");
        }
        if self
            .status_timer
            .as_ref()
            .is_some_and(|t| t.is_event(event).is_some())
        {
            self.status_timer = None;
            self.hide_status(cx);
        }
        if let Event::Actions(actions) = event {
            if self.layers_open {
                self.handle_layers_actions(cx, actions);
            } else if self.model.screen() == Screen::Explore {
                if self.view.button(cx, ids!(layers)).clicked(actions) {
                    self.open_layers(cx);
                }
                if self.view.button(cx, ids!(locate)).clicked(actions)
                    || self.view.button(cx, ids!(locate_active)).clicked(actions)
                {
                    self.ask_location(cx);
                }
                if self.view.button(cx, ids!(compass)).clicked(actions) {
                    self.level_map(cx);
                }
            }
            // The camera moved, by a finger or by the app: the settings
            // keep where it is, and the compass shows when it is not level.
            let map = self.map(cx);
            if let Some((lon, lat, zoom)) = map.viewport_changed(actions) {
                self.model.settings.center = LonLat::new(lon, lat);
                self.model.settings.zoom = zoom;
                self.render(cx);
            }
            if map.tilt_changed(actions).is_some() {
                self.render(cx);
            }
        }
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.ensure_started(cx);
        // The host re-ran the module for a new skin: the map follows it,
        // unless the person chose.
        self.apply_map_theme(cx);
        let step = self.view.draw_walk(cx, scope, walk);
        if self.layers_open {
            // After the draw: a toggle's animator comes up in its default
            // state on its first draw, which would undo a state set before.
            self.sync_switches(cx);
        }
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::isolate_root;

    /// A `MapsView` in an isolate of its own, started, handed to `test`
    /// inside the isolate; torn down in the host's order afterwards. No
    /// network: nothing lands.
    fn with_view(test: impl FnOnce(&mut Cx, &WidgetRef)) {
        let (mut iso, root) = isolate_root();
        iso.entered(|cx| {
            root.handle_event(cx, &Event::Custom(String::new()), &mut Scope::empty());
            test(cx, &root);
            root.borrow_mut::<MapsView>().unwrap().shutdown(cx);
        });
        iso.teardown(root);
    }

    fn back_pressed(cx: &mut Cx, root: &WidgetRef) -> bool {
        // The answer is the event's own cell: a clone would be a copy.
        let event = Event::BackPressed {
            handled: std::cell::Cell::new(false),
        };
        root.handle_event(cx, &event, &mut Scope::empty());
        matches!(event, Event::BackPressed { handled } if handled.get())
    }

    #[test]
    fn the_view_starts_on_explore_with_its_map() {
        with_view(|cx, root| {
            let view = root.borrow::<MapsView>().unwrap();
            assert_eq!(view.model().screen(), Screen::Explore);
            assert!(view.view.widget(cx, ids!(explore)).visible());
            assert!(!view.view.widget(cx, ids!(layers_layer)).visible());
            // The compass is for a map that is not level; this one is.
            assert!(!view.view.widget(cx, ids!(compass)).visible());
            assert!(view.view.widget(cx, ids!(locate)).visible());
            assert!(!view.view.widget(cx, ids!(locate_active)).visible());
            assert_eq!(
                view.view.label(cx, ids!(attribution)).text(),
                "© OpenStreetMap contributors"
            );
            assert_eq!(view.status_text(cx), None);
        });
    }

    #[test]
    fn back_at_explore_is_the_hosts_and_closes_the_layers_sheet_first() {
        with_view(|cx, root| {
            assert!(!back_pressed(cx, root), "nothing to go back from");
            {
                let mut view = root.borrow_mut::<MapsView>().unwrap();
                view.layers_open = true;
                view.render(cx);
                assert!(view.view.widget(cx, ids!(layers_layer)).visible());
            }
            assert!(back_pressed(cx, root));
            let view = root.borrow::<MapsView>().unwrap();
            assert!(!view.layers_open());
            assert!(!view.view.widget(cx, ids!(layers_layer)).visible());
        });
    }

    #[test]
    fn the_locate_button_waits_then_follows_the_fix() {
        with_view(|cx, root| {
            root.borrow_mut::<MapsView>().unwrap().ask_location(cx);
            {
                let view = root.borrow::<MapsView>().unwrap();
                assert_eq!(view.location(), LocationState::Waiting);
                assert_eq!(view.status_text(cx).as_deref(), Some("Locating…"));
            }
            let fix = LocationUpdateEvent {
                lon: -121.8863,
                lat: 37.3382,
                accuracy_m: 12.0,
                altitude_m: None,
                speed_mps: None,
                heading_deg: None,
                time: 0.0,
            };
            root.handle_event(cx, &Event::LocationUpdate(fix), &mut Scope::empty());
            let view = root.borrow::<MapsView>().unwrap();
            assert_eq!(view.location(), LocationState::Active);
            assert_eq!(view.model().fix(), Some(LonLat::new(-121.8863, 37.3382)));
            assert_eq!(view.status_text(cx), None, "found: nothing more to say");
            assert!(view.view.widget(cx, ids!(locate_active)).visible());
            assert!(!view.view.widget(cx, ids!(locate)).visible());
        });
    }

    #[test]
    fn a_location_error_says_why_once_and_lets_the_next_tap_retry() {
        with_view(|cx, root| {
            // An error nobody asked for says nothing.
            root.handle_event(
                cx,
                &Event::LocationError(LocationErrorEvent::PermissionDenied),
                &mut Scope::empty(),
            );
            assert_eq!(root.borrow::<MapsView>().unwrap().status_text(cx), None);
            root.borrow_mut::<MapsView>().unwrap().ask_location(cx);
            root.handle_event(
                cx,
                &Event::LocationError(LocationErrorEvent::PermissionDenied),
                &mut Scope::empty(),
            );
            {
                let view = root.borrow::<MapsView>().unwrap();
                assert_eq!(view.location(), LocationState::Idle);
                assert_eq!(
                    view.status_text(cx).as_deref(),
                    Some("Location is off for OctosMap")
                );
            }
            root.borrow_mut::<MapsView>().unwrap().ask_location(cx);
            assert_eq!(
                root.borrow::<MapsView>().unwrap().location(),
                LocationState::Waiting
            );
        });
    }

    #[test]
    fn a_fix_nobody_asked_for_moves_nothing() {
        with_view(|cx, root| {
            let fix = LocationUpdateEvent {
                lon: 4.9,
                lat: 52.37,
                accuracy_m: 12.0,
                altitude_m: None,
                speed_mps: None,
                heading_deg: None,
                time: 0.0,
            };
            root.handle_event(cx, &Event::LocationUpdate(fix), &mut Scope::empty());
            let view = root.borrow::<MapsView>().unwrap();
            assert_eq!(view.location(), LocationState::Idle);
            assert_eq!(view.model().fix(), None);
        });
    }
}
