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
        octosense_appstore::register_policy_runtime_features();
        crate::view::script_mod(vm);
    }
    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &[
            "storage",
            "net",
            "octos.session.open",
            "octos.session.history",
            "octos.turn.start",
            "octos.turn.interrupt",
        ]
    }
    fn create(
        &self,
        vm: &mut ScriptVm,
        _open: ValidatedOpen,
        _handles: InstanceHandles,
    ) -> InstanceParts {
        let value = script_eval!(vm, { use mod.widgets.* AppHubView {} });
        let root = WidgetRef::script_from_value(vm, value);
        if let Some(mut view) = root.borrow_mut::<AppHubView>() {
            view.open_agent(vm.cx_mut());
        }
        let shutdown_root = root.clone();
        InstanceParts {
            root: root.clone(),
            executor: Box::new(HubExecutor { root }),
            shutdown: Box::new(move |vm| {
                if let Some(mut view) = shutdown_root.borrow_mut::<AppHubView>() {
                    view.shutdown(vm.cx_mut());
                }
            }),
        }
    }
}
/// App Hub's read tools ([`crate::ai`]), answered from the catalog the view
/// shows; installing stays on App Hub's own screens.
struct HubExecutor {
    root: WidgetRef,
}
impl ServiceExecutor for HubExecutor {
    fn manifest(&self) -> ServiceManifest {
        crate::ai::manifest()
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        let result = self
            .root
            .borrow::<AppHubView>()
            .map(|view| crate::ai::answer(view.snapshot(), call))
            .unwrap_or_else(|| ToolResult::unavailable(&call.call_id, "App Hub is gone"));
        ExecOutcome::Done(result)
    }
}
