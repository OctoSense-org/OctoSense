//! Hidden native acceptance host for ordinary contained Splash charts.
//! Run with MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=0, then inspect /help.
//! Optional --script=<path> loads a local UI-only fixture; no host service or
//! account broker is registered by this example.
use makepad_widgets::*;

app_main!(App, font_set: International);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.title: "Native chart acceptance"
                window.inner_size: vec2(1050 760)
                body +: { card := Splash { width: Fill height: Fill } }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
struct App {
    #[live]
    ui: WidgetRef,
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let card = self.ui.splash(cx, ids!(card));
        card.set_policy(cx, Some(Vec::new()), Some(2_000_000));
        let custom_script =
            std::env::args().find_map(|arg| arg.strip_prefix("--script=").map(str::to_owned));
        let source = match custom_script {
            Some(path) => std::fs::read_to_string(path).expect("read --script fixture"),
            None => include_str!("fixtures/charts.splash").to_owned(),
        };
        card.set_text(cx, &source);
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        octosense_shell::charts::register(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
