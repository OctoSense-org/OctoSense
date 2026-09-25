//! Phone-sized native design preview. Installation uses OCTOSENSE_APP_DATA;
//! launch actions are logged here. Use the OctoSense shell to open apps.
pub use makepad_widgets;
use makepad_widgets::*;
app_main!(App);
script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.title: "App Hub · Native preview"
                window.inner_size: vec2(406, 820)
                body +: {padding: 0 margin: 0 spacing: 0 hub := AppHubView{}}
            }
        }
    }
}
#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
}
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        octosense_app_hub_app::view::script_mod(vm);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::Actions(actions) = event {
            for action in actions {
                if let Some(action) = action.as_widget_action() {
                    if let octosense_app_hub_app::AppHubAction::Launch(id) = action.cast() {
                        log!("apphub-preview: launch {id}");
                    }
                }
            }
        }
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
