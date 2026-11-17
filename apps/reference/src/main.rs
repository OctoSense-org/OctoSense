pub use makepad_widgets;
use makepad_widgets::*;

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.title: "Reference"
                window.inner_size: vec2(520, 360)
                body := ReferenceView {}
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        octosense_reference::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

#[cfg(test)]
mod desktop_style_tests {
    use super::*;

    #[test]
    fn the_stock_and_a_desktop_sheet_evaluate_without_script_errors() {
        use makepad_widgets::desktop_style::{install, DesktopStyle, StyleSheet};
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.init_cx_os();
        cx.with_vm(|vm| {
            makepad_widgets::makepad_platform::script::script_mod(vm);
            // A phone's sheets and the stock one have no pointer token.
            vm.bx.captured_errors = Some(Vec::new());
            App::script_mod(vm);
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "{errors:?}");
            for dark in [false, true] {
                // What the shell sends: the Mac's sheet with the pointer token.
                let mut sheet = StyleSheet::load_with_appearance(DesktopStyle::Macos, dark);
                sheet.theme.push_str("\nmod.theme.ui_pointer = true\ntrue\n");
                install(vm, sheet);
                vm.bx.captured_errors = Some(Vec::new());
                vm.with_reload(App::script_mod);
                let errors = vm.take_errors();
                assert!(errors.is_empty(), "{errors:?}");
            }
        });
    }
}
