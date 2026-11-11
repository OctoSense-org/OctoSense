//! The focused card's chat: a virtualized transcript and a separate, fixed
//! composer. Like octoscode-app's conversation column, typing changes only
//! the native editor; it never realizes the whole L0 conversation per key.
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*
    mod.widgets.CardChat = set_type_default() do #(CardChat::register_widget(vm)) {
        width: Fill height: Fill flow: Down spacing: 0
        transcript := PortalList {
            width: Fill height: Fill flow: Down drag_scrolling: true auto_tail: true
            Message := View {
                width: Fill height: Fit flow: Down spacing: 6
                padding: Inset{left: 24 right: 24 top: 14 bottom: 18}
                speaker := Label {
                    width: Fill height: Fit
                    draw_text.text_style: theme.font_bold{font_size: 12.0}
                    draw_text.color: theme.color_text_placeholder
                }
                body := Label {
                    width: Fill height: Fit
                    draw_text.wrap: Words
                    draw_text.text_style: theme.font_regular{font_size: 15.0 line_spacing: 1.35}
                    draw_text.color: theme.color_text
                }
            }
            Person := RoundedView {
                width: Fill height: Fit flow: Down spacing: 5
                margin: Inset{left: 44 right: 18 top: 8 bottom: 14}
                padding: 14
                draw_bg +: {color: #3668e81c border_radius: 16}
                speaker := Label {width: Fill height: Fit draw_text.text_style: theme.font_bold{font_size: 12}}
                body := Label {width: Fill height: Fit draw_text.wrap: Words draw_text.text_style: theme.font_regular{font_size: 15 line_spacing: 1.4}}
            }
            // A Mac's rows: 13-point text, and a person's message that hugs
            // its words.
            MacMessage := View {
                width: Fill height: Fit flow: Down spacing: 2
                padding: Inset{left: 16 right: 16 top: 6 bottom: 6}
                speaker := Label {
                    width: Fill height: Fit padding: 0
                    draw_text.text_style: theme.font_bold{font_size: 9.75}
                    draw_text.color: theme.color_text
                }
                body := Label {
                    width: Fill height: Fit padding: 0
                    draw_text.wrap: Words
                    draw_text.text_style: theme.font_regular{font_size: 9.75 line_spacing: 1.3}
                    draw_text.color: theme.color_text
                }
            }
            MacPerson := View {
                width: Fill height: Fit flow: Right align: Align{x: 1.0}
                padding: Inset{left: 64 right: 16 top: 6 bottom: 6}
                bubble := RoundedView {
                    width: Fit max_width: 380 height: Fit flow: Down
                    padding: Inset{left: 10 right: 10 top: 6 bottom: 6}
                    draw_bg +: {color: #8080802c border_radius: 5}
                    speaker := Label {visible: false width: Fit height: Fit padding: 0 text: ""}
                    body := Label {
                        width: Fit max_width: 360 height: Fit padding: 0
                        draw_text.wrap: Words
                        draw_text.text_style: theme.font_regular{font_size: 9.75 line_spacing: 1.3}
                        draw_text.color: theme.color_text
                    }
                }
            }
        }
        // A Mac's composer: a field that grows from 28 points, its focus ring
        // outside its edge (the rect keeps 3 points around the well), and
        // Send; over it, why asking is off and the button that fixes it.
        mac_dock := View {
            visible: false
            width: Fill height: Fit flow: Down spacing: 6
            padding: Inset{left: 13 right: 16 top: 8 bottom: 9}
            mac_row := View {
                visible: false
                width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                padding: Inset{left: 3}
                mac_status := Label {
                    width: Fill height: Fit padding: 0
                    draw_text.wrap: Words
                    draw_text.text_style: theme.font_regular{font_size: 9.0 line_spacing: 1.2}
                    draw_text.color: theme.color_text_placeholder
                    text: ""
                }
                mac_reply := ButtonFlat {
                    visible: false
                    width: Fit height: 28 margin: 0 padding: Inset{left: 12 right: 12}
                    text: "View Updated Email"
                    draw_bg +: {color: #80808024 color_hover: #80808038 color_down: #8080804c color_focus: #80808024 border_size: 0 border_radius: 3.5}
                    draw_text +: {text_style: theme.font_regular{font_size: 9.75}
                        color: theme.color_text color_hover: theme.color_text color_down: theme.color_text color_focus: theme.color_text}
                }
                mac_fix := ButtonFlat {
                    visible: false
                    width: Fit height: 28 margin: 0 padding: Inset{left: 12 right: 12}
                    text: "Allow\u{2026}"
                    draw_bg +: {color: #x0071e3 color_hover: #x0071e3 color_down: #x005bbf color_focus: #x0071e3 border_size: 0 border_radius: 3.5}
                    draw_text +: {text_style: theme.font_regular{font_size: 9.75}
                        color: #ffffff color_hover: #ffffff color_down: #ffffff color_focus: #ffffff}
                }
            }
            mac_form := View {
                width: Fill height: Fit flow: Right spacing: 5 align: Align{y: 1.0}
                mac_input := TextInputFlat {
                    width: Fill height: Fit min_height: 34 max_height: 100 margin: 0
                    padding: Inset{left: 11 right: 11 top: 9 bottom: 9}
                    is_multiline: true
                    submit_on_enter: true
                    empty_text: "Ask about this card\u{2026}"
                    draw_bg +: {
                        ring: instance(vec4(0.0, 0.48, 1.0, 0.5))
                        well: instance(vec4(1.0, 1.0, 1.0, 0.85))
                        edge: instance(vec4(0.5, 0.5, 0.5, 0.35))
                        off: instance(0.0)
                        pixel: fn() {
                            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                            // The ring stands outside the well: a fill under a clear well would tint it.
                            sdf.box(1.5, 1.5, self.rect_size.x - 3.0, self.rect_size.y - 3.0, 4.25)
                            sdf.stroke(vec4(self.ring.rgb, self.ring.w * self.focus * (1.0 - self.off)), 3.0)
                            sdf.box(3.0, 3.0, self.rect_size.x - 6.0, self.rect_size.y - 6.0, 3.5)
                            sdf.fill_keep(vec4(self.well.rgb, self.well.w * (1.0 - self.off * 0.7)))
                            sdf.stroke(vec4(self.edge.rgb, self.edge.w * (1.0 - self.off * 0.5)), 1.0)
                            return sdf.result
                        }
                    }
                    draw_text.text_style: theme.font_regular{font_size: 9.75 line_spacing: 1.25}
                    draw_cursor.color: theme.color_focus
                    draw_selection +: {
                        color: theme.color_bg_highlight_inline color_hover: theme.color_bg_highlight_inline
                        color_focus: theme.color_bg_highlight_inline color_down: theme.color_bg_highlight_inline
                        color_2: vec4(-1.0) color_2_hover: vec4(-1.0) color_2_focus: vec4(-1.0) color_2_down: vec4(-1.0)
                    }
                }
                mac_send := ButtonFlat {
                    width: Fit height: 28 margin: Inset{bottom: 3} padding: Inset{left: 14 right: 14}
                    text: "Send"
                    draw_bg +: {color: #x0071e3 color_hover: #x0071e3 color_down: #x005bbf color_focus: #x0071e3 color_disabled: #80808024 border_size: 0 border_radius: 3.5}
                    draw_text +: {text_style: theme.font_regular{font_size: 9.75}
                        color: #ffffff color_hover: #ffffff color_down: #ffffff color_focus: #ffffff color_disabled: #80808099}
                }
            }
        }
        dock := View {
            width: Fill height: Fit flow: Down spacing: 8
            padding: Inset{left: 14 right: 14 top: 8 bottom: 14}
            reply := ButtonFlat {visible: false width: Fill height: 44 margin: 0 text: "View updated email →"}
            enable := ButtonFlat {
                visible: false width: Fill height: 44 margin: 0 text: "Enable assistant…"
                draw_bg +: {color: #3668e8 color_hover: #2854c4 color_down: #2148ad border_size: 0 border_radius: 12}
                draw_text +: {color: #ffffff color_hover: #ffffff color_down: #ffffff}
            }
            status := Label {
                width: Fill height: Fit
                draw_text.text_style: theme.font_regular{font_size: 12.0}
                draw_text.color: theme.color_text_placeholder
                text: "Ask about this card or request a change"
            }
            form := RoundedView {
                width: Fill height: Fit flow: Right spacing: 8
                align: Align{y: 1.0}
                padding: 8
                draw_bg.color: theme.color_bg_container
                draw_bg.border_radius: 18.0
                input := TextInputFlat {
                    // Size the viewport from the selected font's real line layout,
                    // while longer drafts retain their full scrollable content.
                    width: Fill height: Fit min_height: 62 max_height: 122 margin: 0 padding: 8
                    is_multiline: true
                    empty_text: "Message the app…"
                    draw_bg +: {pixel: fn() {return vec4(0.0)}}
                    draw_text.text_style: theme.font_regular{font_size: 15.0 line_spacing: 1.25}
                    draw_cursor.color: #3668e8
                    draw_selection.color: #3668e840
                }
                send := ButtonFlat {
                    width: 44 height: 44 margin: 0 padding: 0 text: "↑"
                    draw_bg +: {color: #3668e8 color_hover: #2854c4 color_down: #2148ad border_size: 0 border_radius: 22}
                    draw_text +: {color: #ffffff color_hover: #ffffff color_down: #ffffff text_style: theme.font_bold{font_size: 20}}
                }
            }
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq)]
struct MessageRow { speaker: String, text: String }

/// What a Mac's button over the composer does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fix {
    /// Ask for the first-use permission.
    Allow,
    /// Asking was turned off: the page where it is turned on.
    Settings,
    /// No model yet.
    Providers,
}

/// Whether a model answers here: only a build that hosts the kernel has one.
#[cfg(kernel)]
fn provider_ready() -> bool {
    crate::system_chat::provider_configured(octosense_ai_host::kernel::profile().as_deref())
}
#[cfg(not(kernel))]
fn provider_ready() -> bool {
    false
}

/// A Mac's words for a card's chat that cannot be used, and the fix.
fn mac_blocked(name: &str, consent: crate::agents::Access, account_valid: bool, model: bool) -> Option<(String, Option<(&'static str, Fix)>)> {
    use crate::agents::Access;
    if !account_valid {
        return Some(("This card\u{2019}s account is signed out.".into(), None));
    }
    match consent {
        Access::NotAsked => Some((format!("{name} needs your permission to use its assistant."), Some(("Allow\u{2026}", Fix::Allow)))),
        Access::Off => Some((format!("You turned off the assistant for {name}."), Some(("Open Settings\u{2026}", Fix::Settings)))),
        Access::Allowed if !model => Some(("OctoSense needs an AI model before it can answer.".into(), Some(("Open AI Providers\u{2026}", Fix::Providers)))),
        Access::Allowed => None,
    }
}

/// Split long answers into paragraph rows so even a single long model turn
/// can be virtualized. Row identity is stable until the transcript changes.
fn message_rows(snapshot: &serde_json::Value) -> Vec<MessageRow> {
    let mut rows = Vec::new();
    for entry in snapshot["entries"].as_array().into_iter().flatten() {
        let role = match entry["role"].as_str() {
            Some("user") => "You", Some("model") => "Assistant", _ => "Notice",
        };
        for (index, paragraph) in entry["text"].as_str().unwrap_or_default().split("\n\n").filter(|s| !s.trim().is_empty()).enumerate() {
            rows.push(MessageRow { speaker: if index == 0 { role.into() } else { String::new() }, text: paragraph.into() });
        }
    }
    rows
}

/// One presentation-only patch, shared by application and contrast checks.
fn input_style_patch(vm: &mut ScriptVm) -> ScriptValue {
    script_eval!(vm, {
        use mod.prelude.widgets.*
        let styled = TextInputFlat {
            draw_text +: {text_style: theme.font_regular{font_size: 15.0 line_spacing: 1.25}
                color: theme.color_text color_hover: theme.color_text color_focus: theme.color_text
                color_empty: theme.color_text_placeholder color_empty_hover: theme.color_text_placeholder color_empty_focus: theme.color_text_placeholder}
            draw_cursor.color: theme.color_focus
            draw_selection +: {
                color: theme.color_bg_highlight_inline color_hover: theme.color_bg_highlight_inline
                color_focus: theme.color_bg_highlight_inline color_down: theme.color_bg_highlight_inline
                color_2: vec4(-1.0) color_2_hover: vec4(-1.0) color_2_focus: vec4(-1.0) color_2_down: vec4(-1.0)
            }
        }
        {draw_text: styled.draw_text, draw_cursor: styled.draw_cursor, draw_selection: styled.draw_selection}
    })
}

struct NativeStyle {
    regular: TextStyle,
    bold: TextStyle,
    text: Vec4f,
    placeholder: Vec4f,
    face: Vec4f,
}

#[derive(Script, Widget)]
pub struct CardChat {
    #[deref] view: View,
    #[rust] rows: Vec<MessageRow>,
    #[rust] generation: Option<(u64, u64, Option<String>, bool, crate::agents::Access)>,
    #[rust] pending: Option<String>,
    #[rust] answering: bool,
    #[rust] available: bool,
    #[rust] ink: Option<Vec4f>,
    #[rust] style_revision: u64,
    #[rust] native_style: Option<NativeStyle>,
    #[rust] reply_requested: bool,
    #[rust] publisher: String,
    /// A Mac's tree is the one in use.
    #[rust] mac_on: bool,
    /// The script was applied again: the trees' visibility with it.
    #[rust] tree_dirty: bool,
    /// Take the keyboard after the next draw (the Chat tab was chosen).
    #[rust] want_focus: bool,
    #[rust] fix: Option<Fix>,
    #[rust] providers_requested: bool,
    /// A Mac: the card's assistant can be asked now.
    #[rust] usable_now: bool,
}

impl ScriptHook for CardChat {
    fn on_after_apply(&mut self, _vm: &mut ScriptVm, apply: &Apply, _scope: &mut Scope, _value: ScriptValue) {
        if !apply.is_eval() { self.ink = None; self.style_revision = 0; self.tree_dirty = true; }
    }
}

impl CardChat {
    /// The composer in use.
    fn input(&self, cx: &Cx) -> TextInputRef {
        if self.mac_on { self.view.text_input(cx, ids!(mac_input)) } else { self.view.text_input(cx, ids!(input)) }
    }

    /// Choose a Mac's tree or the other styles', keeping what was typed.
    fn choose_tree(&mut self, cx: &mut Cx) {
        let on = crate::system_chat::view::mac::active();
        if !on && !self.mac_on {
            self.tree_dirty = false;
            return;
        }
        if on == self.mac_on && !self.tree_dirty { return; }
        self.tree_dirty = false;
        if on != self.mac_on {
            let draft = self.input(cx).text();
            self.mac_on = on;
            self.input(cx).set_text(cx, &draft);
        }
        self.view.widget(cx, ids!(mac_dock)).set_visible(cx, on);
        self.view.widget(cx, ids!(dock)).set_visible(cx, !on);
        self.ink = None;
        self.generation = None;
    }

    /// The Chat tab was chosen: the keyboard goes to the field.
    pub fn want_keyboard(&mut self) {
        self.want_focus = true;
    }

    pub fn take_providers_request(&mut self) -> bool { std::mem::take(&mut self.providers_requested) }

    /// A Mac's colours, from the text colour in force.
    fn mac_ink(&mut self, cx: &mut Cx, ink: Vec4f) {
        let dark = ink.x > 0.5;
        let hint = crate::shell::alpha(ink, if dark { 0.72 } else { 0.68 });
        let (ring, well, edge) = if dark {
            (crate::shell::alpha(crate::shell::rgb(10, 132, 255), 0.6), crate::shell::alpha(crate::shell::rgb(255, 255, 255), 0.07), crate::shell::alpha(ink, 0.2))
        } else {
            (crate::shell::alpha(crate::shell::rgb(0, 122, 255), 0.5), crate::shell::alpha(crate::shell::rgb(255, 255, 255), 0.85), crate::shell::alpha(ink, 0.18))
        };
        let mut input = self.view.widget(cx, ids!(mac_input));
        script_apply_eval!(cx, input, {
            draw_bg +: {ring: #(ring) well: #(well) edge: #(edge)}
            draw_text +: {color: #(ink) color_hover: #(ink) color_focus: #(ink) color_empty: #(hint) color_empty_hover: #(hint) color_empty_focus: #(hint)}
        });
        let mut reply = self.view.widget(cx, ids!(mac_reply));
        script_apply_eval!(cx, reply, {draw_text +: {color: #(ink) color_hover: #(ink) color_down: #(ink) color_focus: #(ink)}});
        if let Some(mut label) = self.view.label(cx, ids!(mac_status)).borrow_mut() { label.draw_text.color = hint; }
        self.view.redraw(cx);
    }

    pub fn set_ink(&mut self, cx: &mut Cx, ink: Vec4f) {
        self.choose_tree(cx);
        self.apply_selected_style(cx);
        let ink = self.native_style.as_ref().map_or(ink, |style| style.text);
        if self.ink == Some(ink) { return; }
        self.ink = Some(ink);
        if self.mac_on {
            self.mac_ink(cx, ink);
            return;
        }
        let face = self.native_style.as_ref().map_or_else(|| {
            if ink.x + ink.y + ink.z < 1.5 { crate::shell::rgb(240, 243, 248) } else { crate::shell::rgb(35, 39, 48) }
        }, |style| style.face);
        let mut form = self.view.widget(cx, ids!(form));
        script_apply_eval!(cx, form, {draw_bg +: {color: #(face)}});
        let mut input = self.view.widget(cx, ids!(input));
        let hint = self.native_style.as_ref().map_or_else(|| crate::shell::alpha(ink, 0.55), |style| style.placeholder);
        if self.native_style.is_none() {
            script_apply_eval!(cx, input, {draw_text +: {color: #(ink) color_hover: #(ink) color_focus: #(ink) color_empty: #(hint) color_empty_hover: #(hint) color_empty_focus: #(hint)}});
        }
        if let Some(mut label) = self.view.label(cx, ids!(status)).borrow_mut() { label.draw_text.color = hint; }
        self.view.redraw(cx);
    }
    fn apply_selected_style(&mut self, cx: &mut Cx) {
        let (revision, _) = crate::glance_style::current(cx);
        if revision == self.style_revision { return; }
        let input = self.view.widget(cx, ids!(input));
        let send = self.view.widget(cx, ids!(send));
        let enable = self.view.widget(cx, ids!(enable));
        let reply = self.view.widget(cx, ids!(reply));
        self.native_style = crate::glance_style::with_native_theme(cx, |vm| {
            // Eval applies presentation fields only. It never rebinds the
            // widget source or reapplies draft text, focus, rows or handlers.
            let mut input = input;
            let patch = input_style_patch(vm);
            input.script_apply(vm, &Apply::Eval, &mut Scope::empty(), patch);
            for (mut button, size) in [(send, 20.0), (enable, 14.0)] {
                let patch = script_eval!(vm, {
                    use mod.prelude.widgets.*
                    let styled = ButtonFlat {
                        draw_text +: {text_style: theme.font_bold{font_size: #(size)}
                            color: theme.color_text_on_accent color_hover: theme.color_text_on_accent color_down: theme.color_text_on_accent}
                        draw_bg +: {color: theme.color_focus color_hover: theme.color_focus color_down: theme.color_focus}
                    }
                    {draw_text: styled.draw_text, draw_bg: styled.draw_bg}
                });
                button.script_apply(vm, &Apply::Eval, &mut Scope::empty(), patch);
            }
            // The navigation action remains secondary in the selected style.
            let mut reply = reply;
            let patch = script_eval!(vm, {
                use mod.prelude.widgets.*
                let styled = ButtonFlat {draw_text.text_style: theme.font_regular{font_size: 14.0}}
                {draw_text: styled.draw_text, draw_bg: styled.draw_bg}
            });
            reply.script_apply(vm, &Apply::Eval, &mut Scope::empty(), patch);
            let theme = vm.module(id!(theme));
            let regular = vm.bx.heap.value(theme, id!(font_regular).into(), NoTrap);
            let regular = TextStyle::script_from_value(vm, regular);
            let bold = vm.bx.heap.value(theme, id!(font_bold).into(), NoTrap);
            let bold = TextStyle::script_from_value(vm, bold);
            let mut color = |key: LiveId| {
                let value = vm.bx.heap.value(theme, key.into(), NoTrap);
                Vec4f::script_from_value(vm, value)
            };
            NativeStyle { regular, bold, text: color(id!(color_text)),
                placeholder: color(id!(color_text_placeholder)), face: color(id!(color_bg_container)) }
        });
        if let (Some(style), Some(mut label)) = (self.native_style.as_ref(), self.view.label(cx, ids!(status)).borrow_mut()) {
            label.draw_text.text_style.font_family = style.regular.font_family.clone();
        }
        self.style_revision = revision;
        self.ink = None;
        self.view.redraw(cx);
    }

    pub fn has_unsent_input(&self, cx: &Cx) -> bool { !self.input(cx).text().trim().is_empty() }
    pub fn take_reply_request(&mut self) -> bool { std::mem::take(&mut self.reply_requested) }
    /// Rebind transcript/status while retaining the person's unsubmitted text.
    pub fn refresh(&mut self) { self.generation = None; }
    pub fn reset(&mut self, cx: &mut Cx) {
        self.rows.clear();
        self.reply_requested = false;
        self.generation = None;
        self.pending = None;
        self.answering = false;
        self.available = false;
        self.fix = None;
        self.input(cx).set_text(cx, "");
    }

    pub fn sync(&mut self, cx: &mut Cx, session: &mut crate::glance_card::L0Session) {
        self.publisher = session.app.clone();
        let access = session.chat_access();
        let consent = crate::agents::access(&self.publisher);
        let generation = (crate::glance_chat::generation(), crate::mail_card::generation(), access.0, access.1, consent);
        if self.generation.as_ref() == Some(&generation) { return; }
        self.generation = Some(generation);
        if self.mac_on {
            self.sync_mac(cx, session, consent);
            return;
        }
        self.view.widget(cx, ids!(enable)).set_visible(cx, session.account_valid()
            && consent == crate::agents::Access::NotAsked);
        let first = self.rows.is_empty();
        let status = match session.chat_snapshot() {
            Ok(snapshot) => {
                self.rows = message_rows(&snapshot);
                self.answering = snapshot["status"] == "answering";
                self.available = snapshot["status"] != "unavailable";
                if self.answering { "Thinking…".to_string() }
                else if self.available && session.mail_reply().is_some_and(|d| d["body_origin"] == "model_chat") { "Reply updated · saved".to_string() }
                else if self.available && session.mail_reply().is_some() { "Changes save to your reply".to_string() }
                else if self.available { "Ask this app about the card".to_string() }
                else { "This card's conversation is unavailable".to_string() }
            }
            Err(error) => { self.available = false; self.rows.clear(); error }
        };
        let mail = session.mail_reply();
        self.view.widget(cx, ids!(reply)).set_visible(cx, self.available && mail.is_some_and(|d| d["body_origin"] == "model_chat"));
        let input = self.view.text_input(cx, ids!(input));
        let placeholder = if mail.is_some() { "Change the time, tone or wording…" } else { "Message the app…" };
        if input.empty_text() != placeholder { input.set_empty_text(cx, placeholder.into()); }
        if self.rows.is_empty() && mail.is_some() && self.available {
            self.rows.push(MessageRow {speaker: "Your reply workspace".into(), text: "Ask to change the time, tone or wording. Your saved reply is in Email, where you can edit it and review before sending.".into()});
        } else if self.rows.is_empty() && self.available {
            self.rows.push(MessageRow {speaker: "This card’s assistant".into(), text: "Ask about the card. What the assistant can change depends on the app’s available tools.".into()});
        }
        self.view.label(cx, ids!(status)).set_text(cx, &status);
        if first { self.view.portal_list(cx, ids!(transcript)).scroll_to_end(cx); }
        self.update_send(cx);
        self.view.redraw(cx);
    }

    /// A Mac: rows in plain words, a disabled field with its reason and its
    /// fix when the card's assistant cannot be asked.
    fn sync_mac(&mut self, cx: &mut Cx, session: &mut crate::glance_card::L0Session, consent: crate::agents::Access) {
        let name = crate::approvals::sheet::app_label(&self.publisher);
        let first = self.rows.is_empty();
        let mut note = String::new();
        match session.chat_snapshot() {
            Ok(snapshot) => {
                self.rows = message_rows(&snapshot);
                self.answering = snapshot["status"] == "answering";
                self.available = snapshot["status"] != "unavailable";
            }
            Err(_) => {
                self.available = false;
                self.answering = false;
                self.rows.clear();
            }
        }
        let mail = session.mail_reply();
        // The demo Mail card answers with its canned reply and no model.
        let model = crate::glance_chat::answers_without_model() || provider_ready();
        let blocked = mac_blocked(&name, consent, session.account_valid(), model);
        let usable = self.available && blocked.is_none();
        self.fix = blocked.as_ref().and_then(|(_, fix)| fix.map(|(_, fix)| fix));
        if let Some((why, fix)) = &blocked {
            note = why.clone();
            if let Some((label, _)) = fix {
                self.view.button(cx, ids!(mac_fix)).set_text(cx, label);
            }
        } else if !self.available {
            note = "This conversation isn\u{2019}t available.".into();
        } else if self.answering {
            note = "Thinking\u{2026}".into();
        } else if mail.is_some_and(|d| d["body_origin"] == "model_chat") {
            note = "Reply updated and saved.".into();
        } else if mail.is_some() {
            note = "Changes save to your reply.".into();
        }
        let reply = self.available && mail.is_some_and(|d| d["body_origin"] == "model_chat");
        // The alert is up and asks the same thing: one question, one place to answer it.
        let has_fix = self.fix.is_some_and(|fix| fix != Fix::Allow || !crate::approvals::consent_prompt_up());
        self.view.widget(cx, ids!(mac_fix)).set_visible(cx, has_fix);
        self.view.widget(cx, ids!(mac_reply)).set_visible(cx, reply && !has_fix);
        self.view.label(cx, ids!(mac_status)).set_text(cx, &note);
        self.view.widget(cx, ids!(mac_row)).set_visible(cx, !note.is_empty() || has_fix || reply);
        let input = self.view.text_input(cx, ids!(mac_input));
        let placeholder = if mail.is_some() { "Change the time, tone or wording\u{2026}".to_string() } else { "Ask about this card\u{2026}".to_string() };
        if input.empty_text() != placeholder { input.set_empty_text(cx, placeholder); }
        if let Some(mut field) = input.borrow_mut() {
            field.set_is_read_only(cx, !usable);
        }
        let off = if usable { 0.0_f64 } else { 1.0 };
        let mut well = self.view.widget(cx, ids!(mac_input));
        script_apply_eval!(cx, well, {draw_bg +: {off: #(off)}});
        if !usable && cx.has_key_focus(input.area()) {
            cx.set_key_focus(Area::Empty);
        }
        if self.rows.is_empty() && self.available {
            let (title, text) = if mail.is_some() {
                ("Your Reply", "Ask to change the time, tone or wording. Your saved reply is in Email, where you can edit it and review it before sending.")
            } else {
                ("Ask About This Card", "Ask a question about this card, or ask for a change.")
            };
            self.rows.push(MessageRow { speaker: title.into(), text: text.into() });
        }
        if first { self.view.portal_list(cx, ids!(transcript)).scroll_to_end(cx); }
        self.usable_now = usable;
        self.update_send(cx);
        self.view.redraw(cx);
    }

    fn update_send(&self, cx: &mut Cx) {
        if self.mac_on {
            let disabled = !self.usable_now || self.answering || self.input(cx).text().trim().is_empty();
            self.view.button(cx, ids!(mac_send)).set_disabled(cx, disabled);
            return;
        }
        let disabled = !self.available || self.answering || self.view.text_input(cx, ids!(input)).text().trim().is_empty();
        self.view.button(cx, ids!(send)).set_disabled(cx, disabled);
    }

    pub fn take_submit(&mut self) -> Option<String> { self.pending.take() }

    pub fn submitted(&mut self, cx: &mut Cx, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.input(cx).set_text(cx, "");
                self.view.portal_list(cx, ids!(transcript)).scroll_to_end(cx);
                self.generation = None;
            }
            Err(error) if self.mac_on => {
                self.view.label(cx, ids!(mac_status)).set_text(cx, &crate::system_chat::view::mac::plain_notice(&error));
                self.view.widget(cx, ids!(mac_row)).set_visible(cx, true);
            }
            Err(error) => self.view.label(cx, ids!(status)).set_text(cx, &error),
        }
        self.view.redraw(cx);
    }
}

impl Widget for CardChat {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        if self.view.button(cx, ids!(enable)).clicked(&actions) {
            if let Some(app) = crate::agents::find(&self.publisher) {
                crate::agents::ask(&app);
                cx.hide_text_ime(); cx.set_key_focus(Area::Empty);
                self.generation = None;
            }
        }
        if self.view.button(cx, ids!(reply)).clicked(&actions) || self.view.button(cx, ids!(mac_reply)).clicked(&actions) { self.reply_requested = true; }
        if self.view.button(cx, ids!(mac_fix)).clicked(&actions) {
            match self.fix {
                Some(Fix::Allow) => {
                    if let Some(app) = crate::agents::find(&self.publisher) {
                        crate::agents::ask(&app);
                        self.generation = None;
                    }
                }
                Some(Fix::Settings) => crate::approvals::open_settings(),
                Some(Fix::Providers) => self.providers_requested = true,
                None => {}
            }
        }
        let input = self.input(cx);
        if input.changed(&actions).is_some() {
            // Local editing only. No host storage, model calls, or L0 parsing.
            self.update_send(cx);
        }
        let clicked = self.view.button(cx, if self.mac_on { ids!(mac_send) } else { ids!(send) }).clicked(&actions);
        // A Mac sends on Return (Shift+Return breaks the line).
        let submitted = input.returned(&actions).is_some_and(|(_, modifiers)| modifiers.logo || modifiers.control || self.mac_on);
        let can = !self.mac_on || self.usable_now;
        if (clicked || submitted) && can && self.available && !self.answering {
            let text = input.text();
            if !text.trim().is_empty() { self.pending = Some(text); }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.as_portal_list().borrow_mut() {
                list.set_item_range(cx, 0, self.rows.len());
                while let Some(id) = list.next_visible_item(cx) {
                    let Some(row) = self.rows.get(id) else { continue };
                    // A Mac: a person's bubble and the assistant's text need
                    // no name over them.
                    let (template, speaker) = if self.mac_on {
                        match row.speaker.as_str() {
                            "You" => (id!(MacPerson), ""),
                            "Assistant" | "Notice" => (id!(MacMessage), ""),
                            other => (id!(MacMessage), other),
                        }
                    } else {
                        (if row.speaker == "You" {id!(Person)} else {id!(Message)}, row.speaker.as_str())
                    };
                    let item = list.item(cx, id, template);
                    item.label(cx, ids!(speaker)).set_text(cx, speaker);
                    item.label(cx, ids!(speaker)).set_visible(cx, !speaker.is_empty());
                    if self.mac_on && row.speaker == "Notice" {
                        item.label(cx, ids!(body)).set_text(cx, &crate::system_chat::view::mac::plain_notice(&row.text));
                    } else {
                        item.label(cx, ids!(body)).set_text(cx, &row.text);
                    }
                    if let Some(style) = &self.native_style {
                        if let Some(mut label) = item.label(cx, ids!(body)).borrow_mut() {
                            if label.draw_text.text_style.font_family != style.regular.font_family { label.draw_text.text_style.font_family = style.regular.font_family.clone(); }
                        }
                        if let Some(mut label) = item.label(cx, ids!(speaker)).borrow_mut() {
                            if label.draw_text.text_style.font_family != style.bold.font_family { label.draw_text.text_style.font_family = style.bold.font_family.clone(); }
                        }
                    }
                    if let Some(ink) = self.ink {
                        if let Some(mut label) = item.label(cx, ids!(body)).borrow_mut() { label.draw_text.color = ink; }
                        let speaker_ink = if self.mac_on { ink } else { crate::shell::alpha(ink, 0.65) };
                        if let Some(mut label) = item.label(cx, ids!(speaker)).borrow_mut() { label.draw_text.color = speaker_ink; }
                    }
                    item.draw_all(cx, scope);
                }
            }
        }
        if std::mem::take(&mut self.want_focus) && self.mac_on && self.usable_now {
            self.view.text_input(cx, ids!(mac_input)).set_key_focus(cx);
        }
        DrawStep::done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Mac says why a card cannot be asked, in one sentence, with its fix.
    #[test]
    fn a_macs_blocked_chat_has_a_reason_and_a_fix() {
        use crate::agents::Access;
        let (why, fix) = mac_blocked("Mail", Access::NotAsked, true, true).unwrap();
        assert_eq!(why, "Mail needs your permission to use its assistant.");
        assert_eq!(fix, Some(("Allow\u{2026}", Fix::Allow)));
        assert_eq!(mac_blocked("Mail", Access::Off, true, true).unwrap().1, Some(("Open Settings\u{2026}", Fix::Settings)));
        assert_eq!(mac_blocked("Mail", Access::Allowed, true, false).unwrap().1, Some(("Open AI Providers\u{2026}", Fix::Providers)));
        assert_eq!(mac_blocked("Mail", Access::Allowed, true, true), None);
        assert_eq!(mac_blocked("Mail", Access::Allowed, false, true).unwrap().1, None, "signing in is not this field's to fix");
    }

    #[test]
    fn selected_glance_style_keeps_native_chat_and_draft_with_readable_selection() {
        use makepad_widgets::makepad_draw::cx_draw::CxDraw;
        fn contrast(a: Vec4f, b: Vec4f) -> f64 {
            let luminance = |v: Vec4f| {
                let component = |n: f32| { let n = n as f64; if n <= 0.04045 {n / 12.92} else {((n + 0.055) / 1.055).powf(2.4)} };
                0.2126 * component(v.x) + 0.7152 * component(v.y) + 0.0722 * component(v.z)
            };
            let (a,b) = (luminance(a), luminance(b)); (a.max(b)+0.05)/(a.min(b)+0.05)
        }
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let widget = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            desktop_style::install(vm, desktop_style::StyleSheet::load(desktop_style::DesktopStyle::Omarchy));
            vm.with_reload(makepad_widgets::script_mod);
            super::script_mod(vm);
            let value = script_eval!(vm, {use mod.widgets.* CardChat{}});
            WidgetRef::script_from_value(vm,value)
        });
        let input = widget.text_input(&cx, ids!(input));
        let identity = input.widget_uid();
        let source = input.script_source();
        input.set_text(&mut cx, "Same unsent question — 保留");
        input.set_selection(&mut cx, makepad_draw::text::selection::Selection {
            anchor: makepad_draw::text::selection::Cursor {index:5, prefer_next_row:false},
            cursor: makepad_draw::text::selection::Cursor {index:11, prefer_next_row:false},
        });
        let mut focused = false;
        widget.borrow_mut::<CardChat>().unwrap().rows = vec![MessageRow {speaker:"Assistant".into(),text:"Retained answer".into()}];
        let pass = DrawPass::new(&mut cx); let mut list = DrawList2d::new(&mut cx);
        for dark in [false,true,false] {
            let sheet = desktop_style::StyleSheet::load_with_appearance(desktop_style::DesktopStyle::Android,dark);
            crate::glance_style::select(&mut cx,&sheet);
            widget.borrow_mut::<CardChat>().unwrap().set_ink(&mut cx,crate::shell::rgb(255,255,255));
            let (revision,_) = crate::glance_style::current(&mut cx);
            crate::glance_style::select(&mut cx,&sheet);
            assert_eq!(crate::glance_style::current(&mut cx).0,revision,"no repeated restyle per frame");
            let size = dvec2(380.,330.); pass.set_size(&mut cx,size);
            for _ in 0..2 {
                let event = DrawEvent::default(); let mut draw = CxDraw::new(&mut cx,&event); let mut draw = Cx2d::new(&mut draw);
                draw.begin_pass(&pass,Some(1.)); list.begin_always(&mut draw); draw.begin_root_turtle(size,Layout::default());
                widget.draw_walk_all(&mut draw,&mut Scope::empty(),Walk::fixed(size.x,size.y));
                draw.end_turtle(); list.end(&mut draw); draw.end_pass(&pass);
            }
            assert_eq!(widget.text_input(&cx,ids!(input)).widget_uid(),identity);
            assert_eq!(input.script_source(),source,"presentation patch must not retarget native event ownership");
            assert_eq!(input.text(),"Same unsent question — 保留");
            assert_eq!((input.selection().anchor.index,input.selection().cursor.index),(5,11));
            if focused { assert!(cx.has_key_focus(input.area()), "theme switching retains the active editor"); }
            else {
                cx.set_key_focus(input.area());
                // The platform commits pending focus after event dispatch.
                cx.send_trigger(input.area(), Trigger{id:id!(glance_style_focus),from:Area::Empty});
                cx.handle_triggers();
                assert!(cx.has_key_focus(input.area()));
                focused = true;
            }
            let chat = widget.borrow::<CardChat>().unwrap();
            assert_eq!(chat.rows[0].text,"Retained answer");
            let style = chat.native_style.as_ref().unwrap();
            assert!(contrast(style.text,style.face)>=4.5,"typed text against native field");
            assert!(contrast(style.placeholder,style.face)>=3.0,"placeholder remains distinguishable");
            assert_eq!(widget.label(&cx,ids!(status)).borrow().unwrap().draw_text.text_style.font_family_id(),style.regular.font_family_id());
            crate::glance_style::with_native_theme(&mut cx,|vm| {
                vm.bx.captured_errors = Some(Vec::new());
                let patch = input_style_patch(vm).as_object().unwrap();
                let selection = vm.bx.heap.value(patch,id!(draw_selection).into(),NoTrap).as_object().unwrap();
                for key in [id!(color),id!(color_hover),id!(color_focus),id!(color_down)] {
                    let value = vm.bx.heap.value(selection,key.into(),NoTrap);
                    let color = Vec4f::script_from_value(vm,value);
                    assert!(contrast(style.text,color)>=4.5,"selected text at {key:?}");
                }
                assert!(vm.take_errors().is_empty(),"native presentation patch evaluates cleanly");
            }).unwrap();
            let area = input.area().rect(&cx);
            assert!(area.pos.y>=0. && area.pos.y+area.size.y<=size.y && area.size.y>=40.);
            assert_eq!(cx.with_vm(|vm|desktop_style::current(vm).unwrap().name),"omarchy");
        }
    }

    #[test]
    fn styled_composer_keeps_both_short_rows_visible_and_long_drafts_scrollable() {
        use makepad_widgets::makepad_draw::cx_draw::CxDraw;
        use makepad_draw::text::selection::Cursor;
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let widget = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            desktop_style::install(vm, desktop_style::StyleSheet::load(desktop_style::DesktopStyle::Android));
            vm.with_reload(makepad_widgets::script_mod);
            super::script_mod(vm);
            let value = script_eval!(vm, {use mod.widgets.* CardChat{}});
            WidgetRef::script_from_value(vm, value)
        });
        let input = widget.text_input(&cx, ids!(input));
        let identity = input.widget_uid();
        let pass = DrawPass::new(&mut cx);
        let mut list = DrawList2d::new(&mut cx);
        let mut draw = |cx: &mut Cx, height| {
            let size = dvec2(360.0, height);
            pass.set_size(cx, size);
            for _ in 0..2 {
                let event = DrawEvent::default();
                let mut draw = CxDraw::new(cx, &event);
                let mut draw = Cx2d::new(&mut draw);
                draw.begin_pass(&pass, Some(1.0));
                list.begin_always(&mut draw);
                draw.begin_root_turtle(size, Layout::default());
                widget.draw_walk_all(&mut draw, &mut Scope::empty(), Walk::fixed(size.x, size.y));
                draw.end_turtle(); list.end(&mut draw); draw.end_pass(&pass);
            }
        };
        for dark in [false, true, false] {
            let sheet = desktop_style::StyleSheet::load_with_appearance(desktop_style::DesktopStyle::Android, dark);
            crate::glance_style::select(&mut cx, &sheet);
            widget.borrow_mut::<CardChat>().unwrap().set_ink(&mut cx, crate::shell::rgb(0, 0, 0));
            // Use the exact presentation object applied to the native editor,
            // so the measurement shares its selected font and line spacing.
            let measured = crate::glance_style::with_native_theme(&mut cx, |vm| {
                let patch = input_style_patch(vm).as_object().unwrap();
                let text = vm.bx.heap.value(patch, id!(draw_text).into(), NoTrap);
                DrawText::script_from_value(vm, text)
            }).unwrap();
            for height in [740.0, 330.0, 740.0] {
                input.set_text(&mut cx, "");
                draw(&mut cx, height);
                input.take_key_focus(&mut cx);
                cx.send_trigger(input.area(), Trigger {id:id!(composer_test_focus), from:Area::Empty});
                cx.handle_triggers();
                let short = "Keep this calendar question\nunsent.";
                widget.handle_event(&mut cx, &Event::TextInput(TextInputEvent {input:short.into(), ..Default::default()}), &mut Scope::empty());
                draw(&mut cx, height);
                assert_eq!(input.text(), short, "native input must reach the actual composer");
                let area = input.area().rect(&cx);
                let layout = measured.layout(&mut cx, 0.0, 0.0, Some((area.size.x - 16.0) as f32), true, Align::default(), short);
                assert_eq!(layout.rows.len(), 2);
                let caret = input.cursor_rect_in_absolute(&cx).unwrap();
                let first = &layout.rows[0];
                let last = layout.rows.last().unwrap();
                let row_delta = ((last.origin_in_lpxs.y - last.ascender_in_lpxs) - (first.origin_in_lpxs.y - first.ascender_in_lpxs)) as f64;
                let first_top = caret.pos.y - row_delta;
                eprintln!("composer dark={dark} viewport={height} field={} content={} first_top={} clip_top={} last_bottom={} clip_bottom={}", area.size.y, layout.size_in_lpxs.height, first_top, area.pos.y+8.0, caret.pos.y+caret.size.y, area.pos.y+area.size.y-8.0);
                assert!((caret.size.y - (last.ascender_in_lpxs - last.descender_in_lpxs) as f64).abs() < 0.1, "measurement and actual editor line metrics agree");
                assert!(first_top >= area.pos.y + 8.0 - 0.1, "first line is clipped after typing at its end: {area:?}, {caret:?}");
                assert!(caret.pos.y + caret.size.y <= area.pos.y + area.size.y - 8.0 + 0.1);
                assert!(area.size.y <= 122.0 && area.pos.y + area.size.y <= height);
                assert_eq!(input.widget_uid(), identity);
                assert!(cx.has_key_focus(input.area()));

                let long = "A complete long draft must stay editable.\n".repeat(24);
                input.set_text(&mut cx, "");
                widget.handle_event(&mut cx, &Event::TextInput(TextInputEvent {input:long.clone(), ..Default::default()}), &mut Scope::empty());
                draw(&mut cx, height);
                assert_eq!(input.text(), long);
                let area = input.area().rect(&cx);
                assert!((area.size.y - 122.0).abs() < 0.1, "long editor stays bounded");
                let caret = input.cursor_rect_in_absolute(&cx).unwrap();
                assert!(caret.pos.y >= area.pos.y + 8.0 - 0.1 && caret.pos.y + caret.size.y <= area.pos.y + area.size.y - 8.0 + 0.1, "last row remains reachable");
                input.set_cursor(&mut cx, Cursor {index:0, prefer_next_row:false}, false);
                draw(&mut cx, height);
                let caret = input.cursor_rect_in_absolute(&cx).unwrap();
                assert!(caret.pos.y >= area.pos.y + 8.0 - 0.1 && caret.pos.y + caret.size.y <= area.pos.y + area.size.y - 8.0 + 0.1, "first row remains reachable");
                assert_eq!(input.text(), long);
                assert!(widget.button(&cx, ids!(send)).area().rect(&cx).pos.y + 44.0 <= height);
            }
        }
    }

    #[test]
    fn transcript_is_virtualized_and_composer_stays_visible_when_keyboard_resizes() {
        use makepad_widgets::makepad_draw::cx_draw::CxDraw;
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let widget = cx.with_vm(|vm| {
            vm.bx.captured_errors = Some(Vec::new());
            makepad_widgets::script_mod(vm);
            super::script_mod(vm);
            let value = script_eval!(vm, {use mod.widgets.* CardChat{}});
            let widget = WidgetRef::script_from_value(vm, value);
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "the production composer must instantiate: {errors:?}");
            widget
        });
        widget.borrow_mut::<CardChat>().unwrap().rows = (0..200).map(|id| MessageRow {
            speaker: "Assistant".into(), text: format!("Message {id}: a long conversation must scroll independently of the editor."),
        }).collect();
        let input = widget.text_input(&cx, ids!(input));
        input.set_text(&mut cx, "Keep this unsent draft");
        let pass = DrawPass::new(&mut cx);
        let mut list = DrawList2d::new(&mut cx);
        let mut draw = |cx: &mut Cx, height| {
            let size = dvec2(380.0, height);
            pass.set_size(cx, size);
            let event = DrawEvent::default();
            let mut draw = CxDraw::new(cx, &event);
            let mut draw = Cx2d::new(&mut draw);
            draw.begin_pass(&pass, Some(1.0));
            list.begin_always(&mut draw);
            draw.begin_root_turtle(size, Layout::default());
            widget.draw_walk_all(&mut draw, &mut Scope::empty(), Walk::fixed(380.0, height));
            draw.end_turtle();
            list.end(&mut draw);
            draw.end_pass(&pass);
        };
        for height in [740.0, 330.0, 740.0] {
            draw(&mut cx, height);
            draw(&mut cx, height);
            let send = widget.button(&cx, ids!(send));
            for area in [input.area(), send.area()] {
                let rect = area.rect(&cx);
                assert!(rect.size.y >= 40.0 && rect.pos.y >= 0.0 && rect.pos.y + rect.size.y <= height, "composer clipped at {height}: {rect:?}");
            }
            let transcript = widget.portal_list(&cx, ids!(transcript));
            assert!(transcript.visible_items() < 30, "only visible message rows should draw");
            transcript.set_first_id_and_scroll(0, 0.0);
            draw(&mut cx, height);
            assert_eq!(transcript.first_id(), 0, "reading earlier messages must not jump to the composer");
            assert_eq!(input.text(), "Keep this unsent draft");
        }
    }
    #[test]
    fn long_answers_become_independently_visible_rows_without_losing_text() {
        let rows = message_rows(&serde_json::json!({"entries":[
            {"role":"user","text":"My question"},
            {"role":"model","text":"First paragraph\n\n第二段\nwith a line break\n\nLast paragraph"}
        ]}));
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].speaker, "You");
        assert_eq!(rows[1].speaker, "Assistant");
        assert!(rows[2].speaker.is_empty());
        assert_eq!(rows[2].text, "第二段\nwith a line break");
    }
}
