//! Internal native wrapper for an admitted developer script instance.
use makepad_widgets::*;
use makepad_app_module::{AppModule, ExecOutcome, InstanceHandles, InstanceParts,
    OpenArgKind, OpenSchema, ServiceExecutor, ValidatedOpen,
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult}};
pub struct StudioModule;
pub static STUDIO_MODULE: StudioModule = StudioModule;
impl AppModule for StudioModule {
    fn id(&self) -> &'static str { "studio-app" }
    fn label(&self) -> &'static str { "App Studio" }
    fn register(&self, vm: &mut ScriptVm) { super::apps::script_mod(vm); }
    fn open_schema(&self) -> OpenSchema { OpenSchema::new(1).arg("instance_id", OpenArgKind::Text, true) }
    fn capabilities(&self) -> &'static [&'static str] { &[] }
    fn create(&self, vm: &mut ScriptVm, open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        let id = open.text("instance_id").unwrap_or_default().to_owned();
        InstanceParts { root: super::apps::create(vm, &id), executor: Box::new(StudioExecutor),
            shutdown: Box::new(move |cx| cx.with_cx_mut(|cx| super::apps::shutdown(cx, &id))) }
    }
}
struct StudioExecutor;
impl ServiceExecutor for StudioExecutor {
    fn manifest(&self) -> ServiceManifest { ServiceManifest::new("studio-app", "App Studio", "An isolated local developer app.") }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(&call.call_id, "Use the scoped studio tools"))
    }
}
