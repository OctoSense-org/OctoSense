//! The Reference app's root widget: the sample any host can seat, under a
//! `Window` (`main.rs`) or in-process in the window manager (`module.rs`).

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ReferenceViewBase = #(ReferenceView::register_widget(vm))

    mod.widgets.ReferenceView = set_type_default() do mod.widgets.ReferenceViewBase {
        width: Fill height: Fill
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

#[derive(Script, ScriptHook, Widget)]
pub struct ReferenceView {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    count: usize,
}

impl MatchEvent for ReferenceView {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.view.button(cx, ids!(increment)).clicked(actions) {
            self.count += 1;
            self.view.label(cx, ids!(count)).set_text(cx, &format!("Count: {}", self.count));
        }
        if let Some(text) = self.view.text_input(cx, ids!(message)).changed(actions) {
            self.view.label(cx, ids!(echo)).set_text(cx, &text);
        }
    }
}

impl Widget for ReferenceView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.match_event(cx, event);
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}
