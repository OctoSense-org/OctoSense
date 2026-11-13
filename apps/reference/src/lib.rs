//! The same Reference view in a standalone window or an embedded app instance.
pub use makepad_widgets;
use makepad_widgets::*;
use makepad_app_module::{
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, OpenSchema,
    ServiceExecutor, ValidatedOpen,
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
};

script_mod! {
    use mod.prelude.widgets.*

    // A desktop style sheet sets `theme.ui_pointer`, and the app then draws for a
    // pointer, the way a Mac app does. A phone's sheets leave it out, and reading
    // a key no sheet set ends the evaluation: hence the `try`. Each value below
    // has a touch form and a desktop form, in one tree of one set of names, so
    // what the person typed survives a change of style.
    let desk = try { theme.ui_pointer == true } catch { false }
    fn by(touch, mac) { if desk { mac } else { touch } }

    // Colours are roles of the sheet, so light and dark both follow from it.
    let ink = theme.color_text
    let page = theme.color_bg_app
    let light = dot(ink, #x36b61200) < 0.5
    let lum = dot(page, #x36b61200)
    let mac = desk && (lum > 0.86 || lum < 0.25)
    let muted = mix(ink, page, 0.26)
    let bar = if mac { if light { #xededf0 } else { #x303033 } } else { if light { page } else { mix(page, theme.color_inset, 0.6) } }

    // The Mac's metrics: `font_size` is three quarters of a point, so 9.75 is its
    // 13 point body, and a field and a push button are 28 points tall. The stock
    // widgets already carry the sheet's faces and corners.
    let Field = if desk {
        TextInput {
            height: 28 margin: 0 padding: Inset{left: 8 right: 8 top: 6 bottom: 6}
            draw_text +: { text_style: theme.font_regular{font_size: 9.75} }
        }
    } else { TextInput {} }
    // Return in the field presses it: a desktop's default button, in the system's blue.
    let PushButton = if desk {
        ButtonFlat {
            width: Fit height: 28 margin: 0 padding: Inset{left: 14 right: 14}
            draw_bg +: {
                border_radius: 3.0 border_size: 0.0
                color: #x0071e3 color_hover: #x006edb color_down: #x0060c0 color_focus: #x0071e3
            }
            draw_text +: {
                color: #xffffff color_hover: #xffffff color_down: #xffffff color_focus: #xffffff
                text_style: theme.font_regular{font_size: 9.75}
            }
        }
    } else { Button {} }
    let Body = if desk {
        Label { padding: 0 clip_y: false draw_text +: { color: ink text_style: theme.font_regular{font_size: 9.75} } }
    } else { Label {} }

    mod.widgets.ReferenceView = set_type_default() do #(ReferenceView::register_widget(vm)) {
        ..mod.widgets.RectView
        width: Fill height: Fill
        // A Mac's form sits on the window's grey, as its dialogs' and the Updater's
        // do, and its fields are the white.
        draw_bg.color: page
        flow: Down padding: by(32, 0)
        pointer: desk

        // A title at the head of the page. A desktop's window already has its name
        // in its title bar, and opens at the form's size (`native_window_hint`).
        head := RectView {
            width: Fill height: by(Fit, 40)
            flow: Down spacing: by(20, 0)
            align: Align{y: by(0.0, 0.5)}
            padding: Inset{left: by(0, 14) right: by(0, 14)}
            show_bg: desk
            draw_bg.color: bar
            title := if desk {
                Label {
                    padding: 0
                    clip_y: false
                    text: "Hello from OctoSense"
                    draw_text +: { color: ink text_style: theme.font_bold{font_size: 9.75} }
                }
            } else {
                Label {
                    text: "Hello from OctoSense"
                    draw_text.text_style.font_size: 24
                }
            }
            description := if desk {
                Label {
                    width: Fill
                    padding: 0
                    clip_y: false
                    max_lines: 1
                    text: "An app running inside OctoSense."
                    draw_text +: { color: muted text_style: theme.font_regular{font_size: 8.25} }
                }
            } else {
                Label {
                    width: Fill
                    draw_text.wrap: Words
                    text: "An app running inside OctoSense."
                }
            }
        }

        // On a desktop, a form in the pane's middle: its key to the left of its
        // field, right aligned, and the lines under it on the field's edge.
        View {
            width: Fill height: by(Fit, Fill)
            flow: Down
            margin: Inset{top: by(20, 0)}
            padding: by(0, 20)
            align: Align{x: by(0.0, 0.5) y: 0.0}
            form := View {
                width: by(Fill, 460) height: Fit
                flow: Down spacing: by(20, 10)
                message_line := View {
                    width: Fill height: Fit
                    flow: Right spacing: 8
                    align: Align{y: by(0.0, 0.5)}
                    message_key := Body {
                        visible: desk
                        width: 64
                        align: Align{x: 1.0}
                        text: "Message:"
                    }
                    message := Field { width: Fill empty_text: "Type a message" }
                }
                echo_line := View {
                    width: Fill height: Fit
                    padding: Inset{left: by(0, 72)}
                    // What the empty form says, in the hint's colour: it is not content yet.
                    echo_hint := Label {
                        visible: false
                        width: Fill
                        padding: 0
                        clip_y: false
                        text: "Your message appears here."
                        draw_text +: { color: muted wrap: Words text_style: theme.font_regular{font_size: 9.75} }
                    }
                    echo := if desk {
                        Label {
                            width: Fill
                            padding: 0
                            clip_y: false
                            text: "Your message appears here."
                            draw_text +: { color: ink wrap: Words text_style: theme.font_regular{font_size: 9.75} }
                        }
                    } else {
                        Label { text: "Your message appears here." }
                    }
                }
                View {
                    width: Fill height: Fit
                    flow: by(Down, Right) spacing: by(20, 10)
                    align: Align{y: by(0.0, 0.5)}
                    padding: Inset{left: by(0, 72)}
                    increment := PushButton { text: "Increment" }
                    count := Body { text: "Count: 0" }
                }
            }
        }
    }
}

#[derive(Script, Widget)]
pub struct ReferenceView {
    #[deref]
    view: View,
    /// A desktop style's sheet: `theme.ui_pointer`.
    #[live]
    pointer: bool,
    #[rust]
    count: usize,
    /// The form's width as last applied, 0 when a restyle has written the
    /// script's value back over it.
    #[rust]
    form_width: f64,
    /// A Mac window opens with its first field ready for typing.
    #[rust]
    focused: bool,
    /// A desktop's form is showing, so a switch to a touch style puts the page back.
    #[rust]
    desk_shown: bool,
    /// A style sheet has written the script's words back over the labels.
    #[rust]
    words_stale: bool,
}

impl ScriptHook for ReferenceView {
    fn on_after_apply(&mut self, _vm: &mut ScriptVm, _apply: &Apply, _scope: &mut Scope, _value: ScriptValue) {
        self.form_width = 0.0;
        self.words_stale = true;
    }
}

impl ReferenceView {
    /// What the person has done so far, on the labels.
    fn show_words(&mut self, cx: &mut Cx) {
        self.view.label(cx, ids!(count)).set_text(cx, &format!("Count: {}", self.count));
        let text = self.view.text_input(cx, ids!(message)).text();
        if !text.is_empty() {
            self.view.label(cx, ids!(echo)).set_text(cx, &text);
        }
        let hint = self.pointer && text.is_empty();
        self.view.widget(cx, ids!(echo_hint)).set_visible(cx, hint);
        self.view.widget(cx, ids!(echo)).set_visible(cx, !hint);
    }

    /// The window's width less its 20-point margins, so the field fills what the
    /// window is given, as a Mac form's does when its window widens; 220 at least.
    fn fit_form(&mut self, cx: &mut Cx, room: f64) {
        let width = if room.is_finite() && room > 1.0 { (room - 40.0).max(220.0) } else { 460.0 };
        if (width - self.form_width).abs() > 0.5 {
            self.form_width = width;
            let mut form = self.view.widget(cx, ids!(form));
            script_apply_eval!(cx, form, { width: #(width) });
            // A restyle leaves visibility as it was, and a process starts on the
            // stock sheet: the key and the head are set here, not by `visible: desk`.
            self.view.widget(cx, ids!(message_key)).set_visible(cx, true);
            self.view.widget(cx, ids!(head)).set_visible(cx, false);
        }
    }
}

impl Widget for ReferenceView {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if self.pointer {
            let room = cx.turtle().inner_rect().size.x;
            self.fit_form(cx, room);
            self.desk_shown = true;
        } else if self.desk_shown {
            self.desk_shown = false;
            self.view.widget(cx, ids!(message_key)).set_visible(cx, false);
            self.view.widget(cx, ids!(head)).set_visible(cx, true);
        }
        if self.words_stale {
            self.words_stale = false;
            self.show_words(cx);
        }
        let step = self.view.draw_walk(cx, scope, walk);
        let field = self.view.text_input(cx, ids!(message));
        if self.pointer && !self.focused {
            self.focused = true;
            field.take_key_focus(cx);
        }
        // A Mac's push button doesn't take the keyboard from its window's field: after a
        // press on Increment the typing still goes to the field, as with AppKit's buttons.
        // (Asked on each draw: a process learns its style after its first one.)
        let area = field.area();
        if self.pointer && !area.is_empty() && cx.has_key_focus(area) && !cx.key_focus_kept()
            && makepad_widgets::desktop_style::is_macos(cx)
        {
            cx.keep_key_focus(area);
        }
        step
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            let message = self.view.text_input(cx, ids!(message));
            let returned = self.pointer && message.returned(actions).is_some();
            if self.view.button(cx, ids!(increment)).clicked(actions) || returned {
                self.count += 1;
                self.view.label(cx, ids!(count)).set_text(cx, &format!("Count: {}", self.count));
            }
            // A single-line field lets the keyboard go on Return. A Mac's keeps it when Return
            // presses the default button: the typing goes on in the field.
            if returned && makepad_widgets::desktop_style::is_macos(cx) {
                message.take_key_focus(cx);
            }
            if let Some(text) = message.changed(actions) {
                self.view.label(cx, ids!(echo)).set_text(cx, &text);
                let hint = self.pointer && text.is_empty();
                self.view.widget(cx, ids!(echo_hint)).set_visible(cx, hint);
                self.view.widget(cx, ids!(echo)).set_visible(cx, !hint);
            }
            // A Mac's field keeps the keyboard on Escape; the other desks' let it go.
            if self.pointer && message.escaped(actions) && !makepad_widgets::desktop_style::is_macos(cx) {
                cx.set_key_focus(Area::Empty);
            }
        }
    }
}

pub struct ReferenceModule;
pub static REFERENCE_MODULE: ReferenceModule = ReferenceModule;

impl AppModule for ReferenceModule {
    fn id(&self) -> &'static str { "reference" }
    fn label(&self) -> &'static str { "Reference" }
    fn register(&self, vm: &mut ScriptVm) { script_mod(vm); }
    fn open_schema(&self) -> OpenSchema { OpenSchema::new(1) }
    fn capabilities(&self) -> &'static [&'static str] { &[] }
    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, {
            use mod.widgets.*
            ReferenceView {}
        });
        InstanceParts {
            root: WidgetRef::script_from_value(vm, value),
            executor: Box::new(ReferenceExecutor),
            shutdown: Box::new(|_| {}),
        }
    }
}

struct ReferenceExecutor;
impl ServiceExecutor for ReferenceExecutor {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("reference", "Reference", "A simple input and counter example.")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(&call.call_id, "Reference has no tools"))
    }
}
