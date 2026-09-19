//! OctosMap as a plain full-window Makepad app. For now a bare map on the
//! hosted archive: `--at lat,lon` centres it somewhere else.

pub use makepad_widgets;
use makepad_widgets::*;
use octosense_maps::HOSTED_TILES;

app_main!(App);

/// Downtown San Jose: outside Europe, where the hosted archive's detail is
/// the open question.
const DEFAULT_CENTER: (f64, f64) = (-121.8863, 37.3382);

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
                    View{width: Fill height: Fill flow: Overlay
                        // `debug_cam`: the camera readout is a workbench tool.
                        map := MapView{width: Fill height: Fill zoom: 15.0 min_zoom: 3.0 debug_cam: false}
                    }
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
fn center_from_args(args: &[String]) -> (f64, f64) {
    args.iter()
        .position(|a| a == "--at")
        .and_then(|i| args.get(i + 1))
        .and_then(|value| {
            let (lat, lon) = value.split_once(',')?;
            Some((lon.trim().parse().ok()?, lat.trim().parse().ok()?))
        })
        .unwrap_or(DEFAULT_CENTER)
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let args: Vec<String> = std::env::args().collect();
        let (lon, lat) = center_from_args(&args);
        let map = self.ui.map_view(cx, ids!(map));
        map.set_source_config(cx, TileSourceConfig::http_archive(HOSTED_TILES));
        map.set_center(cx, lon, lat);
    }

    fn handle_actions(&mut self, _cx: &mut Cx, _actions: &Actions) {}
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
