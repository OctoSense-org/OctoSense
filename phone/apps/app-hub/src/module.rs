use crate::view::AppHubView;
use makepad_app_module::{
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
    *,
};
use makepad_widgets::*;

pub struct AppHubModule;
pub static APP_HUB_MODULE: AppHubModule = AppHubModule;

impl AppModule for AppHubModule {
    fn id(&self) -> &'static str {
        "apphub"
    }
    fn label(&self) -> &'static str {
        "App Hub"
    }
    fn register(&self, vm: &mut ScriptVm) {
        crate::view::script_mod(vm);
    }
    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["storage", "net"]
    }
    fn create(
        &self,
        vm: &mut ScriptVm,
        _open: ValidatedOpen,
        _handles: InstanceHandles,
    ) -> InstanceParts {
        let value = script_eval!(vm, { use mod.widgets.* AppHubView {} });
        let root = WidgetRef::script_from_value(vm, value);
        let shutdown_root = root.clone();
        InstanceParts {
            root,
            executor: Box::new(HubExecutor),
            shutdown: Box::new(move |vm| {
                if let Some(mut view) = shutdown_root.borrow_mut::<AppHubView>() {
                    view.shutdown(vm.cx_mut());
                }
            }),
        }
    }
}
struct HubExecutor;
impl ServiceExecutor for HubExecutor {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("apphub", "App Hub", "Discover and install OctoSense apps")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(
            &call.call_id,
            "Use App Hub to review and install apps",
        ))
    }
}
