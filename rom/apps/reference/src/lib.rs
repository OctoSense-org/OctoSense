//! The same Reference view in a standalone window or an embedded app instance.
pub use makepad_widgets;
use makepad_widgets::*;
use makepad_app_module::{
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, OpenSchema,
    ServiceExecutor, ValidatedOpen,
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
};

script_mod! {
    use mod.prelude.widgets.*
    mod.widgets.ReferenceView = set_type_default() do #(ReferenceView::register_widget(vm)) {
        ..mod.widgets.RectView
        width: Fill height: Fill
        draw_bg.color: theme.color_bg_app
        flow: Down padding: 32 spacing: 20
        title := Label {
            text: "Hello from MakeOS"
            draw_text.text_style.font_size: 24
        }
        description := Label {
            width: Fill
            draw_text.wrap: Words
            text: "An app running inside MakeOS."
        }
        message := TextInput { width: Fill empty_text: "Type a message" }
        echo := Label { text: "Your message appears here." }
        increment := Button { text: "Increment" }
        count := Label { text: "Count: 0" }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct ReferenceView {
    #[deref]
    view: View,
    #[rust]
    count: usize,
}

impl Widget for ReferenceView {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            if self.view.button(cx, ids!(increment)).clicked(actions) {
                self.count += 1;
                self.view.label(cx, ids!(count)).set_text(cx, &format!("Count: {}", self.count));
            }
            if let Some(text) = self.view.text_input(cx, ids!(message)).changed(actions) {
                self.view.label(cx, ids!(echo)).set_text(cx, &text);
            }
        }
    }
}

pub struct ReferenceModule;
pub static REFERENCE_MODULE: ReferenceModule = ReferenceModule;

impl AppModule for ReferenceModule {
    fn id(&self) -> &'static str { "reference" }
    fn label(&self) -> &'static str { "Reference" }
    fn register(&self, vm: &mut ScriptVm) { script_mod(vm); }
    fn open_schema(&self) -> OpenSchema { OpenSchema::new(1) }
    fn capabilities(&self) -> &'static [&'static str] { &[] }
    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, {
            use mod.widgets.*
            ReferenceView {}
        });
        InstanceParts {
            root: WidgetRef::script_from_value(vm, value),
            executor: Box::new(ReferenceExecutor),
            shutdown: Box::new(|_| {}),
        }
    }
}

struct ReferenceExecutor;
impl ServiceExecutor for ReferenceExecutor {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("reference", "Reference", "A simple input and counter example.")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(&call.call_id, "Reference has no tools"))
    }
}
