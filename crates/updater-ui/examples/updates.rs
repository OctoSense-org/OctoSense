//! Isolated, app-owned instrument surface. This example never configures a personal profile.
use makepad_widgets::*;
use octosense_updater_ui as updater;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(UpdateTestApp::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.inner_size: if try { theme.ui_pointer == true } catch { false } { vec2(640, 420) } else { vec2(520, 800) }
                body +: { updater := UpdaterView {} }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
struct UpdateTestApp {
    #[live]
    ui: WidgetRef,
}
impl MatchEvent for UpdateTestApp {}
impl AppMain for UpdateTestApp {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        let cache = std::env::var_os("OCTOSENSE_UPDATER_TEST_CACHE")
            .map(std::path::PathBuf::from)
            .expect("Set OCTOSENSE_UPDATER_TEST_CACHE to an isolated absolute directory");
        updater::configure(cache);
        // `OCTOSENSE_UPDATER_TEST`: the desktop pane, read as the release it names
        // (`updater::pretend`). Without it, the phone's layout.
        let test = std::env::var("OCTOSENSE_UPDATER_TEST").ok();
        if let Some(test) = &test {
            updater::pretend(test);
        }
        makepad_widgets::script_mod(vm);
        if test.is_some() {
            script_eval!(vm, { mod.theme.ui_pointer = true });
        }
        updater::script_mod(vm);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
app_main!(UpdateTestApp);
