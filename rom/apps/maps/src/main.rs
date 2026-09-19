//! OctosMap as a plain full-window Makepad app. Everything that makes it a
//! maps app lives in `MapsView` (`src/view.rs`) and none of it depends on
//! this binary, so `cargo test -p octosense-maps` covers it without a
//! window.

pub use makepad_widgets;
use makepad_widgets::*;
use octosense_maps::geo::LonLat;
use octosense_maps::model::force_skin;
use octosense_maps::view::MapsView;

app_main!(App);

/// The zoom `--at` opens on: a neighbourhood.
const AT_ZOOM: f64 = 15.0;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.title: "OctosMap"
                window.inner_size: vec2(480, 800)
                pass +: { clear_color: theme.color_bg_app }
                body +: {
                    padding: 0 margin: 0 spacing: 0
                    maps := MapsView {}
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    /// `--show place:<query>`: the first result opens when it lands.
    #[rust]
    open_first_result: bool,
    /// `--show directions:<query>`: and then its directions.
    #[rust]
    then_directions: bool,
    /// `--show preview:<query>`: and then a preview, once there is a route.
    #[rust]
    then_preview: bool,
}

/// `<flag> lat,lon`, the order a person reads coordinates in.
fn coordinates_arg(args: &[String], flag: &str) -> Option<LonLat> {
    let value = args.get(args.iter().position(|a| a == flag)? + 1)?;
    let (lat, lon) = value.split_once(',')?;
    Some(LonLat::new(
        lon.trim().parse().ok()?,
        lat.trim().parse().ok()?,
    ))
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // Dev flags for looking at the app in a plain window: `--phone`
        // sizes the window like the phone viewport, `--at lat,lon` opens
        // the map there.
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--phone") {
            // The host gives the app the whole rect; the window's own
            // caption bar would take the top of it.
            let mut win = self.ui.widget(cx, ids!(main_window));
            script_apply_eval!(cx, win, { show_caption_bar: false });
            self.ui
                .window(cx, ids!(main_window))
                .resize(cx, dvec2(402.0, 780.0));
        }
        if let Some(center) = coordinates_arg(&args, "--at") {
            if let Some(mut view) = self.ui.widget(cx, ids!(maps)).borrow_mut::<MapsView>() {
                view.set_initial_camera(center, AT_ZOOM);
            }
        }
        makepad_wm_api::set_title(cx, "OctosMap");
        // `--fix lat,lon`: the device is there, for a desk with no GPS. The
        // locate button is pressed and the fix delivered as the platform
        // delivers one.
        if let Some(fix) = coordinates_arg(&args, "--fix") {
            if let Some(mut view) = self.ui.widget(cx, ids!(maps)).borrow_mut::<MapsView>() {
                view.ask_location(cx);
            }
            let update = LocationUpdateEvent {
                lon: fix.lon,
                lat: fix.lat,
                accuracy_m: 10.0,
                altitude_m: None,
                speed_mps: None,
                heading_deg: None,
                time: 0.0,
            };
            self.ui
                .handle_event(cx, &Event::LocationUpdate(update), &mut Scope::empty());
        }
        // `--show <state>`: open on a state a screenshot wants, without
        // driving the window there by hand. `layers`; `search:<query>`, the
        // results of a search; `place:<query>`, its first result's sheet;
        // `directions:<query>`, the routes to it; `preview:<query>`, a
        // simulated drive of the first of them.
        let show = args
            .iter()
            .position(|a| a == "--show")
            .and_then(|i| args.get(i + 1))
            .cloned();
        if let (Some(state), Some(mut view)) = (
            show,
            self.ui.widget(cx, ids!(maps)).borrow_mut::<MapsView>(),
        ) {
            match state.split_once(':') {
                None if state == "layers" => view.open_layers(cx),
                Some(("search", query)) => view.search_for(cx, query),
                Some(("place", query)) => {
                    view.search_for(cx, query);
                    self.open_first_result = true;
                }
                Some(("directions", query)) => {
                    view.search_for(cx, query);
                    self.open_first_result = true;
                    self.then_directions = true;
                }
                Some(("preview", query)) => {
                    view.search_for(cx, query);
                    self.open_first_result = true;
                    self.then_directions = true;
                    self.then_preview = true;
                }
                _ => log!("maps: --show {state} is not a state"),
            }
        }
    }

    // The view handles its own actions; nothing reaches the app.
    fn handle_actions(&mut self, _cx: &mut Cx, _actions: &Actions) {}
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        // Dev flags for looking at the skins in a plain window: a window of
        // its own has no host palette, so `--dark` forces the dark skin and
        // `--light` the light one. Before the crate's module, which splices
        // the skin's colours when it is evaluated.
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--dark") {
            force_skin(Some(false));
        } else if args.iter().any(|a| a == "--light") {
            force_skin(Some(true));
        }
        makepad_widgets::script_mod(vm);
        makepad_wm_theme::apply(vm);
        octosense_maps::view::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // The window manager asked politely (SUPER+W): go now.
        if let Event::Custom(json) = event {
            if let Some(makepad_wm_api::WmEvent::CloseRequested) =
                makepad_wm_api::WmEvent::parse(json)
            {
                cx.quit();
                return;
            }
        }
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        if self.then_preview && !self.open_first_result {
            if let Some(mut view) = self.ui.widget(cx, ids!(maps)).borrow_mut::<MapsView>() {
                if view.model().directions().is_some() {
                    self.then_preview = false;
                    view.start_navigation(cx, true);
                }
            }
        }
        if self.open_first_result {
            if let Some(mut view) = self.ui.widget(cx, ids!(maps)).borrow_mut::<MapsView>() {
                if !view.model().results.is_empty() {
                    self.open_first_result = false;
                    view.pick_result(cx, 0);
                    if std::mem::take(&mut self.then_directions) {
                        view.open_directions(cx);
                    }
                }
            }
        }
    }
}
