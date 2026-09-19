//! OctosMap as a module: the app the window manager seats in a tile
//! in-process, in an isolate of its own.
//!
//! `register` puts this crate's widget family and root type into the
//! isolate the host prepared; `create` mints one `MapsView{}` root there.
//! The module never opens a socket or spawns a thread itself: the map's
//! tiles, the searches and the routes ride the platform's own HTTP request
//! API, and the fixes its location updates, so it works identically hosted.

use crate::view::MapsView;
use makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult};
use makepad_app_module::*;
use makepad_widgets::*;

pub struct MapsModule;

/// The one linked instance of the module description: immutable, no state.
pub static MAPS_MODULE: MapsModule = MapsModule;

impl AppModule for MapsModule {
    fn id(&self) -> &'static str {
        "maps"
    }

    fn label(&self) -> &'static str {
        "OctosMap"
    }

    fn register(&self, vm: &mut ScriptVm) {
        crate::view::script_mod(vm);
    }

    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }

    fn create(
        &self,
        vm: &mut ScriptVm,
        _open: ValidatedOpen,
        _handles: InstanceHandles,
    ) -> InstanceParts {
        let value = script_eval!(vm, {
            use mod.widgets.*
            MapsView {}
        });
        let root = WidgetRef::script_from_value(vm, value);
        let shutdown_root = root.clone();
        InstanceParts {
            root,
            executor: Box::new(MapsExecutor),
            shutdown: Box::new(move |vm| {
                if let Some(mut view) = shutdown_root.borrow_mut::<MapsView>() {
                    view.shutdown(vm.cx_mut());
                }
            }),
        }
    }

    /// The settings live in the storage jail; the tiles, the searches and
    /// the routes are fetched over HTTP; the puck is the device's location.
    fn capabilities(&self) -> &'static [&'static str] {
        &["storage", "net", "location"]
    }
}

/// No tools yet: the assistant is told to use the app itself.
struct MapsExecutor;

impl ServiceExecutor for MapsExecutor {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new(
            "maps",
            "OctosMap",
            "Maps, places, directions and turn-by-turn navigation.",
        )
    }

    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(
            &call.call_id,
            "Use the OctosMap interface",
        ))
    }
}
