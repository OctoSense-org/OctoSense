//! The Reference app as a module: what the window manager seats in-process
//! where it cannot start the sample as a child process (a phone, the web).

use makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult};
use makepad_app_module::*;
use makepad_widgets::*;

pub struct ReferenceModule;

/// The one linked instance of the module description: immutable, no state.
pub static REFERENCE_MODULE: ReferenceModule = ReferenceModule;

impl AppModule for ReferenceModule {
    fn id(&self) -> &'static str {
        "reference"
    }

    fn label(&self) -> &'static str {
        "Reference"
    }

    fn register(&self, vm: &mut ScriptVm) {
        crate::register(vm);
    }

    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }

    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, {
            use mod.widgets.*
            ReferenceView {}
        });
        let root = WidgetRef::script_from_value(vm, value);
        InstanceParts {
            root,
            executor: Box::new(NoTools),
            shutdown: Box::new(|_vm| {}),
        }
    }

    fn capabilities(&self) -> &'static [&'static str] {
        &[]
    }
}

/// The sample has no tools: on the bus for its window, nothing to call.
struct NoTools;

impl ServiceExecutor for NoTools {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("reference", "Reference", "The MakeOS sample app. It exposes no tools.")
    }

    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(&call.call_id, "reference has no tools"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_describes_itself_and_opens_empty() {
        let m = &REFERENCE_MODULE;
        assert_eq!(m.id(), "reference");
        assert_eq!(m.label(), "Reference");
        assert!(m.open_schema().empty_open().is_ok());
    }
}
