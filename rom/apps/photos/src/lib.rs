pub mod model;
mod ui;
pub mod view;
mod zoom;
use makepad_app_module::{
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, OpenSchema, ServiceExecutor,
    ValidatedOpen,
};
pub use makepad_widgets;
use makepad_widgets::*;
pub use ui::script_mod;

pub struct PhotosModule;
pub static PHOTOS_MODULE: PhotosModule = PhotosModule;
impl AppModule for PhotosModule {
    fn id(&self) -> &'static str {
        "photos"
    }
    fn label(&self) -> &'static str {
        "Photos"
    }
    fn register(&self, vm: &mut ScriptVm) {
        ui::script_mod(vm);
    }
    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["storage"]
    }
    fn create(
        &self,
        vm: &mut ScriptVm,
        _open: ValidatedOpen,
        _handles: InstanceHandles,
    ) -> InstanceParts {
        let value = script_eval!(vm,{use mod.widgets.* PhotosView{}});
        let root = WidgetRef::script_from_value(vm, value);
        let shutdown_view = root.clone();
        InstanceParts {
            root,
            executor: Box::new(PhotosExecutor),
            shutdown: Box::new(move |vm| {
                if let Some(mut view) = shutdown_view.borrow_mut::<view::PhotosView>() {
                    view.stop_playback(vm.cx_mut());
                }
            }),
        }
    }
}
struct PhotosExecutor;
impl ServiceExecutor for PhotosExecutor {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new(
            "photos",
            "Photos",
            "Browse your photo library, albums, people, and memories.",
        )
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(
            &call.call_id,
            "Use the Photos app to manage this library.",
        ))
    }
}
