//! Native Reply pane. One authoritative draft is shared with the Mail chat;
//! typing stages locally and coalesces saves, independently of generated L0.
use makepad_widgets::*;
use crate::mail_card::{Binding, Session};

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    mod.widgets.MailClipBase = #(MailClip::register_widget(vm))
    mod.widgets.MailClip = set_type_default() do mod.widgets.MailClipBase {
        width: Fill height: Fill flow: Down spacing: 10
        padding: Inset{left: 18 right: 18 top: 10 bottom: 12}
        state := Label {width: Fill height: Fit draw_text.text_style: theme.font_regular{font_size: 11} text: "Loading reply…"}
        metadata := View {width: Fill height: Fit flow: Down spacing: 12 padding: Inset{top: 4 bottom: 10}
            to_label := Label {width: Fill height: Fit flow: Right {wrap: true} max_lines: 2 text_overflow: Ellipsis draw_text.text_style: theme.font_regular{font_size: 12}}
            subject_label := Label {width: Fill height: Fit flow: Right {wrap: true} max_lines: 2 text_overflow: Ellipsis draw_text.text_style: theme.font_bold{font_size: 16}}
        }
        recipient_row := View {width: Fill height: 40
            recipient := TextInputFlat {width: Fill height: Fill is_read_only: true empty_text: "Recipient" draw_text.text_style: theme.font_regular{font_size: 12}}
        }
        subject_row := View {width: Fill height: 40
            subject := TextInputFlat {width: Fill height: Fill is_read_only: true empty_text: "Subject" draw_text.text_style: theme.font_bold{font_size: 15}}
        }
        preview := View {width: Fill height: Fill
          content := PortalList {
            width: Fill height: Fill flow: Down drag_scrolling: true
            Paragraph := View {
                width: Fill height: Fit padding: Inset{top: 4 bottom: 12}
                copy := Label {width: Fill height: Fit draw_text.wrap: Words draw_text.text_style: theme.font_regular{font_size: 14 line_spacing: 1.5}}
            }
          }
        }
        editor := View {visible: false width: Fill height: Fill
          body := TextInputFlat {
            width: Fill height: Fill is_multiline: true
            padding: 10 empty_text: "Write your reply…"
            draw_text.text_style: theme.font_regular{font_size: 14 line_spacing: 1.5}
            draw_cursor.color: #3668e8 draw_selection.color: #3668e840
          }
        }
        original := ButtonFlat {margin: 0 width: Fill height: 44 text: "View original email"}
        conflict := View {
            visible: false width: Fill height: Fit flow: Down spacing: 6
            notice := Label {width: Fill height: Fit draw_text.wrap: Words text: "The saved reply changed. Your text is kept. Choose which version to use."}
            choices := View {width: Fill height: 44 flow: Right spacing: 8
                mine := ButtonFlat {margin: 0 width: Fill height: Fill text: "Use my edit"}
                saved := ButtonFlat {margin: 0 width: Fill height: Fill text: "Use saved reply"}
            }
        }
        actions := View {width: Fill height: 48 flow: Right spacing: 8
            edit := ButtonFlat {margin: 0 width: 88 height: Fill text: "Edit"}
            review := ButtonFlat {margin: 0 width: Fill height: Fill text: "Review & send"
                draw_bg +: {color: #3668e8 color_hover: #2854c4 color_down: #2148ad border_size: 0 border_radius: 14}
                draw_text +: {color: #ffffff color_hover: #ffffff color_down: #ffffff}
            }
        }
        hint := Label {width: Fill height: Fit draw_text.wrap: Words draw_text.text_style: theme.font_regular{font_size: 10} text: "Nothing is sent until you approve the final reply."}
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct MailClip {
    #[deref] view: View,
    #[rust] session: Option<Session>,
    #[rust] editing: bool,
    #[rust] original: bool,
    #[rust] paragraphs: Vec<String>,
    #[rust] save_timer: Timer,
    #[rust] ink: Option<Vec4f>,
    #[rust] keyboard: bool,
    #[rust] focus_editor: bool,
    #[rust] review_requested: Option<Binding>,
}

impl MailClip {
    pub fn open(&mut self, cx: &mut Cx, binding: Binding) {
        cx.stop_timer(self.save_timer);
        self.editing = false;
        self.original = false;
        self.focus_editor = false;
        self.review_requested = None;
        self.session = Some(Session::new(binding));
        self.render(cx, true);
        self.view.portal_list(cx, ids!(content)).set_first_id_and_scroll(0, 0.0);
    }
    pub fn flush(&mut self, cx: &mut Cx) -> Result<(), String> {
        cx.stop_timer(self.save_timer);
        let result = self.session.as_mut().ok_or("No reply is open".to_string())?.flush();
        self.render(cx, false);
        result
    }
    pub fn take_review(&mut self) -> Option<Binding> { self.review_requested.take() }
    pub fn review_error(&mut self, cx: &mut Cx, error: &str) {
        self.view.label(cx, ids!(state)).set_text(cx, error);
        self.view.redraw(cx);
    }
    pub fn sync(&mut self, cx: &mut Cx) {
        if self.session.as_ref().is_some_and(Session::moved) {
            self.session.as_mut().unwrap().refresh();
            self.render(cx, true);
        }
    }
    fn render(&mut self, cx: &mut Cx, text: bool) {
        let Some(s) = &self.session else { return; };
        let d = s.snapshot();
        let editable = matches!(d["status"].as_str(), Some("draft" | "awaiting_approval"));
        let conflict = s.error.as_ref().is_some_and(|e| e.contains("revision_conflict"));
        let body_focus = cx.has_key_focus(self.view.text_input(cx, ids!(body)).area());
        for path in [ids!(recipient_row), ids!(subject_row)] { self.view.widget(cx, path).set_visible(cx, self.editing && !self.original && !(self.keyboard && body_focus)); }
        self.view.widget(cx, ids!(metadata)).set_visible(cx, !self.editing || self.original);
        for path in [ids!(original), ids!(hint)] { self.view.widget(cx, path).set_visible(cx, !self.keyboard); }
        let status = s.error.as_deref().unwrap_or_else(|| match d["status"].as_str() {
            Some("accepted") => "SMTP accepted · delivery unconfirmed",
            Some("outcome_unknown") => "Send outcome unknown · check Sent before retrying",
            Some("sending") => "Sending…",
            Some("failed_before_delivery") => "Send failed before delivery · return to review",
            _ if d["body_origin"] == "model_chat" => "Updated from chat · saved",
            _ => "Draft saved",
        });
        self.view.label(cx, ids!(state)).set_text(cx, status);
        self.view.widget(cx, ids!(conflict)).set_visible(cx, conflict);
        self.view.button(cx, ids!(edit)).set_disabled(cx, !editable || self.original);
        self.view.button(cx, ids!(review)).set_disabled(cx, s.error.is_some() || self.original || d["status"] == "sending" || d["status"] == "accepted");
        self.view.button(cx, ids!(edit)).set_text(cx, if self.editing { "Done" } else { "Edit" });
        self.view.button(cx, ids!(original)).set_text(cx, if self.original { "Back to reply" } else { "View original email" });
        for path in [ids!(recipient), ids!(subject)] {
            self.view.text_input(cx, path).set_is_read_only(cx, !self.editing || self.original || !editable);
        }
        self.view.widget(cx, ids!(editor)).set_visible(cx, self.editing && !self.original);
        self.view.widget(cx, ids!(preview)).set_visible(cx, !self.editing || self.original);
        if text {
            let source = if self.original { &d["email"] } else { d };
            let recipient = if self.original { source["address"].as_str().unwrap_or("") } else { d["to"].as_str().unwrap_or("") };
            self.view.label(cx, ids!(to_label)).set_text(cx, &format!("{}  {recipient}", if self.original {"From"} else {"To"}));
            self.view.label(cx, ids!(subject_label)).set_text(cx, source["subject"].as_str().unwrap_or(""));
            for (path, value) in [(ids!(recipient), recipient), (ids!(subject), source["subject"].as_str().unwrap_or("")), (ids!(body), d["body"].as_str().unwrap_or(""))] {
                let input = self.view.text_input(cx, path);
                // No cursor/selection reset for unrelated service notifications.
                if input.text() != value { input.set_text(cx, value); }
            }
            self.paragraphs = source["body"].as_str().unwrap_or("").split("\n\n").map(str::to_owned).collect();
        }
        self.view.redraw(cx);
    }
    pub fn set_ink(&mut self, cx: &mut Cx, ink: Vec4f) {
        if self.ink == Some(ink) { return; }
        self.ink = Some(ink);
        let light = ink.x + ink.y + ink.z < 1.5;
        let face = if light {crate::shell::rgb(243, 246, 250)} else {crate::shell::rgb(35, 39, 48)};
        for path in [ids!(recipient), ids!(subject), ids!(body)] {
            let mut input = self.view.widget(cx, path);
            script_apply_eval!(cx, input, {draw_bg +: {color: #(face) border_radius: 10} draw_text +: {color: #(ink)}});
        }
        for path in [ids!(edit), ids!(original), ids!(mine), ids!(saved)] {
            let mut button = self.view.widget(cx, path);
            script_apply_eval!(cx, button, {draw_bg +: {color: #(face) color_hover: #(face) color_down: #(face) color_focus: #(face) border_size: 0 border_radius: 12} draw_text +: {color: #(ink) color_hover: #(ink) color_down: #(ink) color_focus: #(ink)}});
        }
        for path in [ids!(state), ids!(hint), ids!(notice), ids!(to_label)] {
            if let Some(mut label) = self.view.label(cx, path).borrow_mut() { label.draw_text.color = crate::shell::alpha(ink, 0.72); }
        }
        if let Some(mut label) = self.view.label(cx, ids!(subject_label)).borrow_mut() { label.draw_text.color = ink; }
    }
}
impl Widget for MailClip {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if let Event::VirtualKeyboard(keyboard) = event {
            self.keyboard = matches!(keyboard, VirtualKeyboardEvent::WillShow {height, ..} | VirtualKeyboardEvent::DidShow {height, ..} if *height > 0.0);
            self.render(cx, false);
        }
        if self.save_timer.is_event(event).is_some() { let _ = self.flush(cx); }
        self.sync(cx);
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        if self.editing && !self.original {
            for (path, name) in [(ids!(recipient), "to"), (ids!(subject), "subject"), (ids!(body), "body")] {
                if let Some(text) = self.view.text_input(cx, path).changed(&actions) {
                    if let Some(session) = &mut self.session {
                        if let Err(error) = session.stage(name, &text) { session.error = Some(error); }
                    }
                    cx.stop_timer(self.save_timer);
                    self.save_timer = cx.start_timeout(0.5);
                    self.render(cx, false);
                }
            }
        }
        if self.view.button(cx, ids!(edit)).clicked(&actions) {
            if !self.editing || self.flush(cx).is_ok() {
                self.editing = !self.editing;
                if !self.editing { cx.hide_text_ime(); cx.set_key_focus(Area::Empty); }
                self.render(cx, true);
                self.focus_editor = self.editing;
            }
        }
        if self.view.button(cx, ids!(original)).clicked(&actions) && self.flush(cx).is_ok() {
            self.original = !self.original;
            cx.hide_text_ime(); cx.set_key_focus(Area::Empty);
            self.render(cx, true);
            self.view.portal_list(cx, ids!(content)).set_first_id_and_scroll(0, 0.0);
        }
        for (path, keep) in [(ids!(mine), true), (ids!(saved), false)] {
            if self.view.button(cx, path).clicked(&actions) {
                if let Some(s) = &mut self.session { if let Err(e) = s.resolve_edit(keep) { s.error = Some(e); } }
                self.render(cx, true);
            }
        }
        if self.view.button(cx, ids!(review)).clicked(&actions) && self.flush(cx).is_ok() {
            self.review_requested = self.session.as_ref().map(|s| s.binding.clone());
            self.editing = false;
            self.render(cx, true);
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.sync(cx);
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.as_portal_list().borrow_mut() {
                list.set_item_range(cx, 0, self.paragraphs.len());
                while let Some(id) = list.next_visible_item(cx) {
                    let Some(text) = self.paragraphs.get(id) else { continue; };
                    let item = list.item(cx, id, id!(Paragraph));
                    item.label(cx, ids!(copy)).set_text(cx, text);
                    if let (Some(ink), Some(mut label)) = (self.ink, item.label(cx, ids!(copy)).borrow_mut()) { label.draw_text.color = ink; }
                    item.draw_all(cx, scope);
                }
            }
        }
        if self.focus_editor {
            self.focus_editor = false;
            self.view.text_input(cx, ids!(body)).set_key_focus(cx);
        }
        DrawStep::done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reply_editor_and_review_action_fit_above_the_keyboard() {
        use makepad_widgets::makepad_draw::cx_draw::CxDraw;
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let widget = cx.with_vm(|vm| {
            vm.bx.captured_errors = Some(Vec::new());
            makepad_widgets::script_mod(vm);
            super::script_mod(vm);
            let value = script_eval!(vm, {use mod.widgets.* MailClip{}});
            let widget = WidgetRef::script_from_value(vm, value);
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "production Reply widget: {errors:?}");
            widget
        });
        let input = widget.text_input(&cx, ids!(body));
        input.set_text(&mut cx, "Keep my corrected appointment time\n\nWednesday at 10:00.");
        widget.widget(&cx, ids!(editor)).set_visible(&mut cx, true);
        widget.widget(&cx, ids!(preview)).set_visible(&mut cx, false);
        for path in [ids!(metadata), ids!(recipient_row), ids!(subject_row), ids!(original), ids!(hint)] { widget.widget(&cx, path).set_visible(&mut cx, false); }
        let pass = DrawPass::new(&mut cx);
        let mut list = DrawList2d::new(&mut cx);
        for height in [700.0, 260.0, 700.0] {
            for _ in 0..2 {
                let size = dvec2(380.0, height);
                pass.set_size(&mut cx, size);
                let event = DrawEvent::default();
                let mut draw = CxDraw::new(&mut cx, &event);
                let mut draw = Cx2d::new(&mut draw);
                draw.begin_pass(&pass, Some(1.0)); list.begin_always(&mut draw);
                draw.begin_root_turtle(size, Layout::default());
                widget.draw_walk_all(&mut draw, &mut Scope::empty(), Walk::fixed(380.0, height));
                draw.end_turtle(); list.end(&mut draw); draw.end_pass(&pass);
            }
            for area in [input.area(), widget.button(&cx, ids!(review)).area()] {
                let r = area.rect(&cx);
                assert!(r.size.y >= 44.0 && r.pos.y >= 0.0 && r.pos.y + r.size.y <= height, "clipped at {height}: {r:?}");
            }
            assert!(input.area().rect(&cx).size.y > height - 130.0, "the hidden preview must not compete with the editor for space");
            assert!(input.text().contains("Wednesday at 10:00"));
        }
    }
}
