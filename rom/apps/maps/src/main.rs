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
}

/// `--at lat,lon`, the order a person reads coordinates in.
fn center_from_args(args: &[String]) -> Option<LonLat> {
    let value = args.get(args.iter().position(|a| a == "--at")? + 1)?;
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
        if let Some(center) = center_from_args(&args) {
            if let Some(mut view) = self.ui.widget(cx, ids!(maps)).borrow_mut::<MapsView>() {
                view.set_initial_camera(center, AT_ZOOM);
            }
        }
        makepad_wm_api::set_title(cx, "OctosMap");
        // `--show <state>`: open on a state a screenshot wants, without
        // driving the window there by hand.
        let show = args
            .iter()
            .position(|a| a == "--show")
            .and_then(|i| args.get(i + 1));
        if let (Some(state), Some(mut view)) = (
            show,
            self.ui.widget(cx, ids!(maps)).borrow_mut::<MapsView>(),
        ) {
            match state.as_str() {
                "layers" => view.open_layers(cx),
                other => log!("maps: --show {other} is not a state"),
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
    }
}
