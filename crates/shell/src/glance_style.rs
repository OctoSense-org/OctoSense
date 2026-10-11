//! Host-selected presentation for resident Glance surfaces. The shell's own
//! chrome keeps its ambient theme; app source is never rewritten for styling.
use makepad_widgets::*;
use std::rc::Rc;

#[derive(Default)]
struct Selected {
    sheet: Option<Rc<desktop_style::StyleSheet>>,
    revision: u64,
    native_vm: Option<widget_async::SplashVmId>,
}

pub(crate) fn select(cx: &mut Cx, sheet: &desktop_style::StyleSheet) {
    let selected = cx.global::<Selected>();
    if selected.sheet.as_deref() == Some(sheet) { return; }
    selected.sheet = Some(Rc::new(sheet.clone()));
    selected.revision += 1;
    let old = selected.native_vm.take();
    if let Some(old) = old { cx.free_splash_vm(old); }
    cx.redraw_all();
}

pub(crate) fn current(cx: &mut Cx) -> (u64, Option<Rc<desktop_style::StyleSheet>>) {
    let selected = cx.global::<Selected>();
    (selected.revision, selected.sheet.clone())
}

/// Native fallback chat reads bundled font/color roles in a trusted theme-only
/// isolate, cached once per selected revision. No app source, grants, network,
/// host services or timers run here, and the ambient shell VM stays unchanged.
pub(crate) fn with_native_theme<R>(cx: &mut Cx, f: impl FnOnce(&mut ScriptVm) -> R) -> Option<R> {
    let sheet = cx.global::<Selected>().sheet.clone()?;
    let existing = cx.global::<Selected>().native_vm;
    let vm_id = if let Some(vm_id) = existing { vm_id } else {
        let vm_id = cx.alloc_splash_vm_with_network(false);
        cx.with_script_vm_id_trusted(vm_id, |vm| {
            desktop_style::install(vm, (*sheet).clone());
            vm.with_reload(|vm| {
                // Only the framework's packaged-font resolver is needed.
                script_eval!(vm, {mod.res = {crate_resource: mod.prelude.widgets.crate_resource}});
                makepad_widgets::widgets_mod(vm);
                desktop_style::apply_widgets(vm);
                script_eval!(vm, {mod.res = nil});
            });
        });
        cx.global::<Selected>().native_vm = Some(vm_id);
        vm_id
    };
    Some(cx.with_script_vm_id_trusted(vm_id, f))
}
