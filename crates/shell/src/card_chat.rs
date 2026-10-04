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
                padding: Inset{left: 20 right: 20 top: 10 bottom: 14}
                speaker := Label {
                    width: Fill height: Fit
                    draw_text.text_style: theme.font_bold{font_size: 10.0}
                    draw_text.color: theme.color_text_placeholder
                }
                body := Label {
                    width: Fill height: Fit
                    draw_text.wrap: Words
                    draw_text.text_style: theme.font_regular{font_size: 13.0 line_spacing: 1.35}
                    draw_text.color: theme.color_text
                }
            }
            Person := RoundedView {
                width: Fill height: Fit flow: Down spacing: 5
                margin: Inset{left: 44 right: 18 top: 8 bottom: 14}
                padding: 14
                draw_bg +: {color: #3668e81c border_radius: 16}
                speaker := Label {width: Fill height: Fit draw_text.text_style: theme.font_bold{font_size: 10}}
                body := Label {width: Fill height: Fit draw_text.wrap: Words draw_text.text_style: theme.font_regular{font_size: 13 line_spacing: 1.4}}
            }
        }
        dock := View {
            width: Fill height: Fit flow: Down spacing: 8
            padding: Inset{left: 14 right: 14 top: 8 bottom: 14}
            updated := ButtonFlat {
                visible: false margin: 0 width: Fill height: 44 text: "Reply updated · View reply →"
                draw_bg +: {color: #eaf1ff color_hover: #dce8ff color_down: #d0e0ff border_radius: 12 border_size: 0}
                draw_text +: {color: #2659b7 color_hover: #2659b7 color_down: #2659b7 text_style: theme.font_regular{font_size: 12}}
            }
            status := Label {
                width: Fill height: Fit
                draw_text.text_style: theme.font_regular{font_size: 10.0}
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
                    width: Fill height: 62 margin: 0 padding: 8
                    is_multiline: true
                    empty_text: "Message the app…"
                    draw_bg +: {pixel: fn() {return vec4(0.0)}}
                    draw_text.text_style: theme.font_regular{font_size: 13.0 line_spacing: 1.25}
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

fn composer_height(text: &str, width: f64) -> f64 {
    // Include soft wraps as well as explicit newlines. Height is bounded;
    // TextInput keeps its complete layout/selection and scrolls longer drafts.
    let line_width = (width - 16.0).max(120.0);
    let lines: f64 = text.split('\n').map(|line| {
        let pixels: f64 = line.chars().map(|c| if c.is_ascii() {7.0} else {14.0}).sum();
        (pixels / line_width).ceil().max(1.0)
    }).sum();
    20.0 * lines.clamp(2.0, 5.0) + 22.0
}

#[derive(Script, ScriptHook, Widget)]
pub struct CardChat {
    #[deref] view: View,
    #[rust] rows: Vec<MessageRow>,
    #[rust] generation: Option<(u64, u64, Option<String>, bool)>,
    #[rust] pending: Option<String>,
    #[rust] answering: bool,
    #[rust] available: bool,
    #[rust] ink: Option<Vec4f>,
    #[rust] open_reply: bool,
}

impl CardChat {
    pub fn set_ink(&mut self, cx: &mut Cx, ink: Vec4f) {
        if self.ink == Some(ink) { return; }
        self.ink = Some(ink);
        let face = if ink.x + ink.y + ink.z < 1.5 { crate::shell::rgb(240, 243, 248) } else { crate::shell::rgb(35, 39, 48) };
        let mut form = self.view.widget(cx, ids!(form));
        script_apply_eval!(cx, form, {draw_bg +: {color: #(face)}});
        let mut input = self.view.widget(cx, ids!(input));
        script_apply_eval!(cx, input, {draw_text +: {color: #(ink)}});
        if let Some(mut label) = self.view.label(cx, ids!(status)).borrow_mut() { label.draw_text.color = crate::shell::alpha(ink, 0.7); }
        self.view.redraw(cx);
    }
    pub fn reset(&mut self, cx: &mut Cx) {
        self.rows.clear();
        self.generation = None;
        self.pending = None;
        self.answering = false;
        self.available = false;
        self.open_reply = false;
        self.view.widget(cx, ids!(updated)).set_visible(cx, false);
        self.view.text_input(cx, ids!(input)).set_text(cx, "");
    }

    pub fn sync(&mut self, cx: &mut Cx, session: &mut crate::glance_card::L0Session) {
        let access = session.chat_access();
        let generation = (crate::glance_chat::generation(), crate::mail_card::generation(), access.0, access.1);
        if self.generation.as_ref() == Some(&generation) { return; }
        self.generation = Some(generation);
        let first = self.rows.is_empty();
        let status = match session.chat_snapshot() {
            Ok(snapshot) => {
                self.rows = message_rows(&snapshot);
                self.answering = snapshot["status"] == "answering";
                self.available = snapshot["status"] != "unavailable";
                if self.answering { "Thinking…".to_string() }
                else if self.available && session.mail_reply().is_some() { "Request changes here. Review the saved email in Reply.".to_string() }
                else if self.available { "Ask about this card or request a change".to_string() }
                else { "This card's conversation is unavailable".to_string() }
            }
            Err(error) => { self.available = false; self.rows.clear(); error }
        };
        let mail = session.mail_reply();
        let input = self.view.text_input(cx, ids!(input));
        let placeholder = if mail.is_some() { "Change the time, tone or wording…" } else { "Message the app…" };
        if input.empty_text() != placeholder { input.set_empty_text(cx, placeholder.into()); }
        if self.rows.is_empty() && mail.is_some() && self.available {
            self.rows.push(MessageRow {speaker: "Your reply workspace".into(), text: "Ask to change the time, tone or wording. Your saved email is in Reply, where you can edit it and review before sending.".into()});
        }
        self.view.widget(cx, ids!(updated)).set_visible(cx, mail.is_some_and(|d| d["body_origin"] == "model_chat"));
        self.view.label(cx, ids!(status)).set_text(cx, &status);
        if first { self.view.portal_list(cx, ids!(transcript)).scroll_to_end(cx); }
        self.update_send(cx);
        self.view.redraw(cx);
    }

    fn update_send(&self, cx: &mut Cx) {
        let disabled = !self.available || self.answering || self.view.text_input(cx, ids!(input)).text().trim().is_empty();
        self.view.button(cx, ids!(send)).set_disabled(cx, disabled);
    }

    pub fn take_submit(&mut self) -> Option<String> { self.pending.take() }
    pub fn take_open_reply(&mut self) -> bool { std::mem::take(&mut self.open_reply) }

    pub fn submitted(&mut self, cx: &mut Cx, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.view.text_input(cx, ids!(input)).set_text(cx, "");
                self.view.text_input(cx, ids!(input)).set_height(cx, Size::Fixed(62.0));
                self.view.portal_list(cx, ids!(transcript)).scroll_to_end(cx);
                self.generation = None;
            }
            Err(error) => self.view.label(cx, ids!(status)).set_text(cx, &error),
        }
        self.view.redraw(cx);
    }
}

impl Widget for CardChat {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        if self.view.button(cx, ids!(updated)).clicked(&actions) { self.open_reply = true; }
        let input = self.view.text_input(cx, ids!(input));
        if let Some(text) = input.changed(&actions) {
            // Local editing only. No host storage, model calls, or L0 parsing.
            input.set_height(cx, Size::Fixed(composer_height(&text, input.area().rect(cx).size.x)));
            self.update_send(cx);
        }
        let clicked = self.view.button(cx, ids!(send)).clicked(&actions);
        let submitted = input.returned(&actions).is_some_and(|(_, modifiers)| modifiers.logo || modifiers.control);
        if (clicked || submitted) && self.available && !self.answering {
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
                    let item = list.item(cx, id, if row.speaker == "You" {id!(Person)} else {id!(Message)});
                    item.label(cx, ids!(speaker)).set_text(cx, &row.speaker);
                    item.label(cx, ids!(speaker)).set_visible(cx, !row.speaker.is_empty());
                    item.label(cx, ids!(body)).set_text(cx, &row.text);
                    if let Some(ink) = self.ink {
                        if let Some(mut label) = item.label(cx, ids!(body)).borrow_mut() { label.draw_text.color = ink; }
                        if let Some(mut label) = item.label(cx, ids!(speaker)).borrow_mut() { label.draw_text.color = crate::shell::alpha(ink, 0.65); }
                    }
                    item.draw_all(cx, scope);
                }
            }
        }
        DrawStep::done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_grows_for_wrapped_text_and_stops_before_covering_history() {
        assert_eq!(composer_height("short", 280.0), 62.0);
        assert!(composer_height(&"words ".repeat(18), 250.0) > 62.0);
        assert!(composer_height(&"更改时间".repeat(12), 250.0) > 62.0);
        assert_eq!(composer_height(&"long ".repeat(1000), 250.0), 122.0);
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
