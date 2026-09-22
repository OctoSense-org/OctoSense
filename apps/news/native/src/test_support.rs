//! Scaffolding the isolate tests share (`view.rs`, `module.rs`): a `Cx`
//! with no window, a splash VM of its own with no network and this crate's
//! widget family registered in it — what the module host prepares for an
//! instance — a `NewsView` root minted there, and the host's teardown
//! order: the refs go first, then the isolate.

use crate::module::NEWS_MODULE;
use makepad_app_module::AppModule;
use makepad_widgets::widget_async::{enter_isolate, leave_isolate, SplashVmId};
use makepad_widgets::*;

/// One test's `Cx` and the isolate its root lives in.
pub(crate) struct Isolate {
    pub cx: Cx,
    pub vm_id: SplashVmId,
}

impl Isolate {
    /// A fresh isolate with the crate's widgets registered, as
    /// `NewsModule::register` puts them there for the host.
    pub(crate) fn new() -> Self {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.init_cx_os();
        cx.with_vm(makepad_widgets::script_mod);
        let vm_id = cx.alloc_splash_vm_with_network(false);
        cx.with_script_vm_id_trusted(vm_id, |vm| NEWS_MODULE.register(vm));
        Isolate { cx, vm_id }
    }

    /// `f` with the isolate's VM, as the host calls `register` and `create`.
    pub(crate) fn with_vm<R>(&mut self, f: impl FnOnce(&mut ScriptVm) -> R) -> R {
        self.cx.with_script_vm_id_trusted(self.vm_id, f)
    }

    /// `f` with the isolate entered, as the host runs an instance's events.
    pub(crate) fn entered<R>(&mut self, f: impl FnOnce(&mut Cx) -> R) -> R {
        let entry = enter_isolate(&mut self.cx, self.vm_id);
        let out = f(&mut self.cx);
        leave_isolate(&mut self.cx, entry);
        out
    }

    /// The host's order: the root goes first, then the isolate. Any other
    /// ref into the isolate (an executor) is the caller's to drop before.
    pub(crate) fn teardown(self, root: WidgetRef) {
        drop(root);
        let Isolate { mut cx, vm_id } = self;
        cx.free_splash_vm(vm_id);
    }
}

/// A `NewsView {}` root minted in a fresh isolate, which evaluated it (and
/// the reader its DSL mounts) without script errors.
pub(crate) fn isolate_root() -> (Isolate, WidgetRef) {
    let mut iso = Isolate::new();
    let root = iso.with_vm(|vm| {
        let value = script_eval!(vm, {
            use mod.widgets.*
            NewsView {}
        });
        let root = WidgetRef::script_from_value(vm, value);
        assert!(vm.take_errors().is_empty(), "the isolate evaluated the view and the reader without errors");
        root
    });
    (iso, root)
}
