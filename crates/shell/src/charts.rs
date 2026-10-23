//! Native charts share the ordinary Splash VM, including its app identity,
//! storage, network policy and lifetime. No nested chart-specific host is used.
use makepad_widgets::{widget_async, ScriptVm};

/// Register the chart vocabulary in this VM and every future Splash isolate
/// on this UI thread. Full apps and Glance use the same runtime hook.
pub fn register(vm: &mut ScriptVm) {
    thread_local! {
        static REGISTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if !REGISTERED.with(|done| done.replace(true)) {
        widget_async::register_splash_isolate_mod(chart_vocabulary);
    }
    chart_vocabulary(vm);
    register_feature();
}

/// Compatibility discovery must also work before the first app VM opens.
pub(crate) fn register_feature() {
    #[cfg(any(feature = "app-hub", native_mobile))]
    octosense_appstore::host_api::register_runtime_feature("charts.d3", 1);
}

fn chart_vocabulary(vm: &mut ScriptVm) {
    // Keep the legacy d3.Octoscript host out: it creates a second VM without
    // the containing app's admitted identity and ordinary Splash cleanup.
    makepad_d3::octoscript::script_mod(vm);
    makepad_d3::render3d::draw::script_mod(vm);
    makepad_d3::octoscript::charts::script_mod(vm);
    makepad_d3::octoscript::charts_stat::script_mod(vm);
    makepad_d3::octoscript::charts_hier::script_mod(vm);
    makepad_d3::octoscript::charts_flow::script_mod(vm);
    makepad_d3::octoscript::charts_net::script_mod(vm);
    makepad_d3::octoscript::charts_3d::script_mod(vm);
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_widgets::{script_eval, widget_async::CxSplashVmExt, Cx, ScriptMod};

    #[test]
    fn ordinary_isolates_have_charts_without_a_nested_host() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            let absent = script_eval!(vm, {
                use mod.prelude.widgets.*
                try { d3.BarChart != nil } catch { false }
            });
            assert_eq!(absent.as_bool(), Some(false));
            register(vm);
        });
        // Registering only in the parent VM used to leave contained bodies
        // without third-party widgets. Exercise two independent runtime VMs.
        for _ in 0..2 {
            let isolate = cx.alloc_splash_vm_with_network(false);
            cx.with_script_vm_id(isolate, |vm| {
                let available = script_eval!(vm, {
                    use mod.prelude.widgets.*
                    d3.BarChart != nil && d3.LineChart != nil && d3.Heatmap != nil
                });
                assert_eq!(available.as_bool(), Some(true));
                let nested_host = script_eval!(vm, {
                    try { mod.d3.Octoscript != nil } catch { false }
                });
                assert_eq!(nested_host.as_bool(), Some(false));
                assert!(vm.take_errors().is_empty());
            });
            cx.free_splash_vm(isolate);
        }
    }
}
