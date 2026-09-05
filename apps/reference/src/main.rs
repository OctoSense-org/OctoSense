pub use makepad_widgets;
use makepad_widgets::*;

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.title: "Reference"
                window.inner_size: vec2(520, 360)
                body +: {
                    flow: Down
                    padding: 32
                    spacing: 20
                    title := Label {
                        text: "Hello from MakeOS"
                        draw_text.text_style.font_size: 24
                    }
                    description := Label {
                        text: "A separate app running inside your desktop."
                    }
                    message := TextInput {
                        width: Fill
                        empty_text: "Type a message"
                    }
                    echo := Label { text: "Your message appears here." }
                    increment := Button { text: "Increment" }
                    count := Label { text: "Count: 0" }
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    count: usize,
}

impl MatchEvent for App {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.ui.button(cx, ids!(increment)).clicked(actions) {
            self.count += 1;
            self.ui
                .label(cx, ids!(count))
                .set_text(cx, &format!("Count: {}", self.count));
        }
        if let Some(text) = self.ui.text_input(cx, ids!(message)).changed(actions) {
            self.ui.label(cx, ids!(echo)).set_text(cx, &text);
        }
    }
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
