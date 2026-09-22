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
        handles: InstanceHandles,
    ) -> InstanceParts {
        let value = script_eval!(vm, {
            use mod.widgets.*
            MapsView {}
        });
        let root = WidgetRef::script_from_value(vm, value);
        if let Some(mut view) = root.borrow_mut::<MapsView>() {
            // The instance's disk is its storage jail: the settings and the
            // last camera live there, on every host the same way.
            view.set_storage(vm.cx_mut(), handles.storage);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Screen;
    use crate::test_support::Isolate;

    #[test]
    fn the_module_describes_itself_and_opens_empty() {
        let m = &MAPS_MODULE;
        assert_eq!(m.id(), "maps");
        assert_eq!(m.label(), "OctosMap");
        assert_eq!(m.capabilities(), &["storage", "net", "location"]);
        let schema = m.open_schema();
        assert_eq!(schema.version, 1);
        assert!(schema.empty_open().is_ok(), "no argument is required");
        assert!(
            schema.validate(r#"{"place":"home"}"#, &[]).is_err(),
            "nothing is passed in"
        );
    }

    /// The whole contract without a window manager: an instance in an
    /// isolate of its own, its jail read on the start, no tool to call,
    /// teardown in the host's order.
    #[test]
    fn the_module_mints_its_root_in_a_fresh_isolate() {
        let mut iso = Isolate::new();
        let storage = iso.cx.storage("maps.test");
        let (replies, _upstream) = ReplySink::pair();
        let handles = InstanceHandles {
            scope: InstanceScope::new(1, 1),
            storage,
            viewport: Viewport {
                size: dvec2(400.0, 700.0),
            },
            replies,
        };
        let open = MAPS_MODULE.open_schema().empty_open().unwrap();
        let InstanceParts {
            root,
            executor,
            shutdown,
        } = iso.with_vm(|vm| {
            let parts = MAPS_MODULE.create(vm, open, handles);
            let errors = vm.take_errors();
            assert!(
                errors.is_empty(),
                "the isolate evaluated the view: {errors:?}"
            );
            parts
        });
        assert!(
            root.borrow::<MapsView>().is_some(),
            "the root is a MapsView"
        );
        let manifest = executor.manifest();
        assert_eq!(manifest.id, "maps");
        assert!(manifest.tools.is_empty(), "no tools yet");

        // The first event starts it: Explore, and the jail being read.
        iso.entered(|cx| root.handle_event(cx, &Event::Custom(String::new()), &mut Scope::empty()));
        {
            let view = root.borrow::<MapsView>().unwrap();
            assert_eq!(view.model().screen(), Screen::Explore);
            assert!(view.has_pending_load(), "the jail is read on the start");
        }
        drop(executor);
        iso.with_vm(|vm| shutdown(vm));
        iso.teardown(root);
    }
}
