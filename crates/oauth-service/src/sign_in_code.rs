//! The one-time-code panel of the host's provider sign-in sheet: the code in
//! large type, a Copy button, and an Open button that copies the code and
//! opens the provider's page.
//!
//! The sheet's script binds the panel to its sheet ticket. The code and the
//! page come from [`publish`], which only the host's sign-in worker calls,
//! never from script arguments: a contained app that places this widget can
//! neither show a "code" of its choosing nor make it copy or open anything.
use makepad_widgets::*;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Default, PartialEq)]
struct Published {
    /// Empty for a provider without a device code (Google's browser sign-in).
    code: String,
    url: String,
}

static PUBLISHED: Mutex<Option<HashMap<String, Published>>> = Mutex::new(None);

/// The code and page a sheet ticket shows; `code` may be empty.
pub(crate) fn publish(ticket: &str, code: &str, url: &str) {
    PUBLISHED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(
            ticket.into(),
            Published {
                code: code.into(),
                url: url.into(),
            },
        );
}

/// The ticket's sign-in ended: its panel stops offering the code.
pub(crate) fn retire(ticket: &str) {
    if let Some(map) = PUBLISHED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        map.remove(ticket);
    }
}

fn lookup(ticket: &str) -> Option<Published> {
    PUBLISHED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|map| map.get(ticket).cloned())
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    mod.widgets.SignInCodeBase = #(SignInCode::register_widget(vm))
    mod.widgets.SignInCode = set_type_default() do mod.widgets.SignInCodeBase{
        width: Fill height: Fit flow: Down spacing: 12
        code_box := RoundedView {width: Fill height: Fit padding: Inset{top: 16 bottom: 16 left: 12 right: 12} align: Align{x: 0.5}
            draw_bg +: {color: #xf3f6f9 border_radius: 10.0 border_size: 1.0 border_color: #xd5dce5}
            code := Label {width: Fit text: "" draw_text.color: #x172336 draw_text.text_style: theme.font_code{font_size: 28}}
        }
        View {width: Fill height: Fit flow: Right spacing: 10
            copy := ButtonFlat {width: Fill height: 48 text: "Copy code"
                draw_bg +: {color: #xffffff color_hover: #xf3f6f9 color_down: #xe8edf3 border_size: 1.0 border_color: #xc9d2dd border_radius: 8.0}
                draw_text +: {color: #x172336 color_hover: #x172336 color_down: #x172336 text_style: theme.font_regular{font_size: 13}}
            }
            open := Button {width: Fill height: 48 text: "Open GitHub"
                draw_bg +: {color: #x1f2937 color_hover: #x111827 color_down: #x0b1220 color_focus: #x1f2937 border_size: 0.0 border_radius: 8.0}
                draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff color_focus: #xffffff text_style: theme.font_bold{font_size: 13}}
            }
        }
    }
}

/// Install the panel into Splash. The sign-in sheet is its only meaningful
/// user: without a ticket the host published, it shows and does nothing.
pub fn register() {
    thread_local! { static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    if !DONE.with(|done| done.replace(true)) {
        widget_async::register_splash_isolate_mod(install);
    }
}

fn install(vm: &mut ScriptVm) {
    script_mod(vm);
    script_eval!(vm, {mod.prelude.widgets.SignInCode = mod.widgets.SignInCode});
}

#[derive(Script, ScriptHook, Widget)]
pub struct SignInCode {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    shown: Option<Published>,
    #[rust]
    open_label: String,
    #[rust]
    reset: Timer,
}

impl SignInCode {
    fn show(&mut self, cx: &mut Cx, published: Option<Published>) {
        let has_code = published.as_ref().is_some_and(|p| !p.code.is_empty());
        self.view
            .label(cx, ids!(code))
            .set_text(cx, published.as_ref().map_or("", |p| p.code.as_str()));
        self.view
            .widget(cx, ids!(code_box))
            .set_visible(cx, has_code);
        self.view.widget(cx, ids!(copy)).set_visible(cx, has_code);
        self.view.button(cx, ids!(copy)).set_text(cx, "Copy code");
        self.view
            .button(cx, ids!(open))
            .set_text(cx, &self.open_label);
        self.shown = published;
        self.view.redraw(cx);
    }

    fn copied(&mut self, cx: &mut Cx, text: &str) {
        self.view.button(cx, ids!(copy)).set_text(cx, text);
        cx.stop_timer(self.reset);
        self.reset = cx.start_timeout(2.0);
    }
}

impl Widget for SignInCode {
    fn script_call(
        &mut self,
        vm: &mut ScriptVm,
        method: LiveId,
        args: ScriptValue,
    ) -> ScriptAsyncResult {
        // bind(ticket, open_label): show the ticket's published code; true if
        // the host published one for it.
        if method == live_id!(bind) {
            let arg = |index: usize, vm: &mut ScriptVm| {
                args.as_object().and_then(|obj| {
                    let trap = vm.bx.threads.cur().trap.pass();
                    let value = vm.bx.heap.vec_value(obj, index, trap);
                    vm.bx
                        .heap
                        .cast_to_owned_string(value, "binding the sign-in code")
                })
            };
            let ticket = arg(0, vm).unwrap_or_default();
            if let Some(label) = arg(1, vm).filter(|l| !l.is_empty() && l.len() <= 40) {
                self.open_label = label;
            }
            let published = lookup(&ticket);
            let found = published.is_some();
            vm.with_cx_mut(|cx| self.show(cx, published));
            return ScriptAsyncResult::Return(found.into());
        }
        ScriptAsyncResult::MethodNotFound
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        if self.reset.is_event(event).is_some() {
            self.view.button(cx, ids!(copy)).set_text(cx, "Copy code");
        }
        let Some(shown) = self.shown.clone() else {
            return;
        };
        if self.view.button(cx, ids!(copy)).clicked(&actions) && !shown.code.is_empty() {
            cx.copy_to_clipboard(&shown.code);
            self.copied(cx, "Copied");
        }
        if self.view.button(cx, ids!(open)).clicked(&actions) {
            if !shown.code.is_empty() {
                cx.copy_to_clipboard(&shown.code);
                self.copied(cx, "Code copied");
            }
            cx.open_url(&shown.url, OpenUrlInPlace::No);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}
