//! Rinx's article editor without its Matrix account/publication controller.
//!
//! The widget owns only editing state. Its embedding app owns local storage,
//! repository selection and host-reviewed publication. No account, filesystem,
//! network or GitHub credential is reachable through this widget.
use article_core::{
    document::{Block, BlockKind, Document, Theme},
    editing::EditHistory,
};
use article_makepad::{
    body_selection::{ArticleSelection, SelectionUpdate},
    presentation,
    rich_input::ArticleRichInputWidgetRefExt,
};
use makepad_widgets::makepad_script::ScriptFnRef;
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    let EditorButton = ButtonFlat {
        margin: 0
        draw_bg +: {color: #xf2f5fa color_hover: #xe7edf6 color_down: #xdce6f3 color_focus: #xf2f5fa border_size: 0 border_radius: 8}
        draw_text +: {color: #x28415b color_hover: #x28415b color_down: #x28415b color_focus: #x28415b}
    }
    mod.widgets.MarkdownEditorBase = #(MarkdownEditor::register_widget(vm))
    mod.widgets.MarkdownEditor = set_type_default() do mod.widgets.MarkdownEditorBase {
        width: Fill height: Fill flow: Down spacing: 8
        modes := View {width: Fill height: 44 spacing: 4
            rich_mode := EditorButton {width: Fill height: Fill text: "Write" grab_key_focus: false}
            source_mode := EditorButton {width: Fill height: Fill text: "Markdown" grab_key_focus: false}
            preview_mode := EditorButton {width: Fill height: Fill text: "Preview" grab_key_focus: false}
        }
        formatting := ScrollXView {width: Fill height: 44 flow: Right spacing: 4
            bold := EditorButton {height: 44 text: "Bold" grab_key_focus: false}
            italic := EditorButton {height: 44 text: "Italic" grab_key_focus: false}
            heading := EditorButton {height: 44 text: "Heading" grab_key_focus: false}
            bullet := EditorButton {height: 44 text: "List" grab_key_focus: false}
            quote := EditorButton {height: 44 text: "Quote" grab_key_focus: false}
            paragraph := EditorButton {height: 44 text: "+ Paragraph" grab_key_focus: false}
            table := EditorButton {height: 44 text: "+ Table" grab_key_focus: false}
            code := EditorButton {height: 44 text: "+ Code" grab_key_focus: false}
            math := EditorButton {height: 44 text: "+ Math" grab_key_focus: false}
            link := EditorButton {height: 44 text: "+ Link" grab_key_focus: false}
            image := EditorButton {height: 44 text: "+ Image URL" grab_key_focus: false}
            undo := EditorButton {height: 44 text: "Undo" grab_key_focus: false}
            redo := EditorButton {height: 44 text: "Redo" grab_key_focus: false}
        }
        rich_pane := View {width: Fill height: Fill
            blocks := PortalList {width: Fill height: Fill drag_scrolling: true
                Text := View {width: Fill height: Fit flow: Down padding: Inset{left: 8 right: 8}
                    kind := Label {width: Fill height: Fit padding: 0 draw_text.color: #x576d80 draw_text.text_style.font_size: 10}
                    rich := ArticleRichInput {width: Fill height: Fit is_multiline: true flow: Right{wrap: true}
                        padding: Inset{top: 8 bottom: 12 left: 8 right: 8}
                        draw_text.text_style: theme.font_regular{font_size: 15}
                        draw_bold.text_style: theme.font_bold{font_size: 15}
                        draw_italic.text_style: theme.font_italic{font_size: 15}
                        draw_bold_italic.text_style: theme.font_bold_italic{font_size: 15}
                        draw_bg +: {color: #xffffff color_hover: #xffffff color_focus: #xffffff color_down: #xffffff color_empty: #xffffff color_2: #xffffff color_2_hover: #xffffff color_2_focus: #xffffff color_2_down: #xffffff color_2_empty: #xffffff border_size: 0 border_radius: 6}
                        draw_text +: {color_empty: #x73869a color_empty_hover: #x73869a color_empty_focus: #x73869a}
                        draw_selection +: {color: #x9ab3d440 color_focus: #x9ab3d440}
                    }
                }
            }
        }
        source_pane := View {width: Fill height: Fill visible: false
            markdown := TextInput {width: Fill height: Fill is_multiline: true
                empty_text: "Write Markdown…" padding: 12
                draw_bg +: {color: #xffffff color_hover: #xffffff color_focus: #xffffff color_down: #xffffff color_empty: #xffffff color_2: #xffffff color_2_hover: #xffffff color_2_focus: #xffffff color_2_down: #xffffff color_2_empty: #xffffff border_size: 1 border_color: #xdbe3ec border_radius: 8}
                draw_text +: {color: #x20354b color_hover: #x20354b color_focus: #x20354b color_empty: #x73869a color_empty_hover: #x73869a color_empty_focus: #x73869a}
                draw_text.text_style: theme.font_code{font_size: 14}
                draw_selection +: {color: #x9ab3d440 color_focus: #x9ab3d440}
            }
        }
        preview_pane := ScrollYView {width: Fill height: Fill visible: false
            preview := Html {width: Fill height: Fit padding: 12 selectable: true
                text_style_normal: theme.font_regular{font_size: 15}
                text_style_bold: theme.font_bold{font_size: 15}
                rmath := ArticleMath {} rdiagram := ArticleDiagram {}
                rimage := ArticleImage {} remoji := ArticleEmoji {}
                rcode := ArticleCode {} rcell := ArticleCell {}
            }
        }
        footer := View {width: Fill height: Fit spacing: 8 align: Align{y: 0.5}
            editor_status := Label {width: Fill height: Fit text: "Write · Markdown · Preview" draw_text.color: #x576d80 draw_text.wrap: Words draw_text.text_style.font_size: 11}
            theme := EditorButton {height: 44 text: "Theme" grab_key_focus: false}
        }
    }
}

/// Add the UI vocabulary to contained apps. This grants no host capabilities.
pub fn register() {
    thread_local! { static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    if !DONE.with(|done| done.replace(true)) {
        widget_async::register_splash_isolate_mod(install);
    }
}

pub fn install(vm: &mut ScriptVm) {
    article_makepad::script_mod(vm);
    script_mod(vm);
    // Makepad's public prelude is a snapshot assembled before host mods.
    // Export only the safe composite, not Rinx's internal widget vocabulary.
    script_eval!(vm, {mod.prelude.widgets.MarkdownEditor = mod.widgets.MarkdownEditor});
}

#[derive(Default, PartialEq)]
enum Mode {
    #[default]
    Rich,
    Source,
    Preview,
}

#[derive(Script, ScriptHook, Widget)]
pub struct MarkdownEditor {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    on_change: Option<ScriptFnRef>,
    #[rust]
    document: Document,
    #[rust]
    history: EditHistory,
    #[rust]
    selection: ArticleSelection,
    #[rust]
    selected: usize,
    #[rust]
    mode: Mode,
    #[rust]
    markdown: String,
    #[rust]
    parse_error: Option<String>,
    #[rust]
    initialized: bool,
    #[rust]
    preview_dirty: bool,
}

impl MarkdownEditor {
    fn emit_change(&self, cx: &mut Cx) {
        let Some(handler) = self.on_change.clone() else {
            return;
        };
        let Some(vm_id) = cx.script_ref_vm_id(&self.source) else {
            return;
        };
        let text = cx.with_script_vm_id(vm_id, |vm| {
            ScriptValue::from(vm.bx.heap.new_string_from_str(&self.markdown))
        });
        cx.widget_to_script_call(
            self.widget_uid(),
            NIL,
            self.source.clone(),
            handler,
            &[text],
        );
    }

    fn replace_markdown(&mut self, cx: &mut Cx, text: &str, record: bool) -> Result<(), String> {
        let mut next = Document::from_markdown("", text)?;
        next.theme = self.document.theme;
        next.large_type = self.document.large_type;
        next.compact = self.document.compact;
        if record {
            self.history.checkpoint(&self.document);
        } else {
            self.history.clear();
        }
        self.document = next;
        self.markdown = text.into();
        self.parse_error = None;
        self.selected = 0;
        self.selection.reset();
        self.preview_dirty = true;
        self.view.text_input(cx, ids!(markdown)).set_text(cx, text);
        self.view
            .portal_list(cx, ids!(blocks))
            .set_first_id_and_scroll(0, 0.0);
        self.render(cx);
        Ok(())
    }

    fn render(&mut self, cx: &mut Cx) {
        self.view
            .widget(cx, ids!(rich_pane))
            .set_visible(cx, self.mode == Mode::Rich);
        self.view
            .widget(cx, ids!(source_pane))
            .set_visible(cx, self.mode == Mode::Source);
        self.view
            .widget(cx, ids!(preview_pane))
            .set_visible(cx, self.mode == Mode::Preview);
        self.view
            .widget(cx, ids!(formatting))
            .set_visible(cx, self.mode == Mode::Rich);
        for (path, selected, title) in [
            (ids!(rich_mode), self.mode == Mode::Rich, "Write"),
            (ids!(source_mode), self.mode == Mode::Source, "Markdown"),
            (ids!(preview_mode), self.mode == Mode::Preview, "Preview"),
        ] {
            self.view
                .button(cx, path)
                .set_text(cx, &format!("{}{title}", if selected { "● " } else { "" }));
        }
        if self.mode == Mode::Preview && self.preview_dirty {
            let images = article_makepad::content::Images::default();
            let mut renderer = article_makepad::content::NativeRenderer {
                images: &images,
                size: 15.0,
                ink: self.document.theme.colors().1,
            };
            let html = article_core::markdown_render::render(&self.markdown, &mut renderer)
                .iter()
                .map(|block| article_makepad::content::native_html(&block.html))
                .collect::<String>();
            let mut preview = self.view.html(cx, ids!(preview));
            presentation::style_html(cx, preview.clone(), &self.document);
            preview.set_text(cx, &html);
            self.preview_dirty = false;
        }
        self.view.label(cx, ids!(editor_status)).set_text(
            cx,
            self.parse_error
                .as_deref()
                .unwrap_or(if self.mode == Mode::Rich {
                    "Select text to format · scroll toolbar for more"
                } else if self.mode == Mode::Source {
                    "Exact Markdown source · full document"
                } else {
                    "Rinx preview · remote images stay offline"
                }),
        );
        self.view.redraw(cx);
    }

    fn changed(&mut self, cx: &mut Cx) {
        self.markdown = self.document.markdown();
        self.preview_dirty = true;
        self.parse_error = None;
        self.render(cx);
        self.emit_change(cx);
    }

    fn format(&mut self, cx: &mut Cx, bold: bool) {
        if let Some(selection) = self.selection.selection {
            let (start, _) = selection.ordered();
            let Some(block) = self.document.blocks.get(start.block) else {
                return;
            };
            let flags = block.flags_at(start.byte);
            self.history.checkpoint(&self.document);
            for (index, block) in self.document.blocks.iter_mut().enumerate() {
                if let Some(range) = selection
                    .range(index, block.text.len())
                    .filter(|r| !r.is_empty())
                {
                    let _ = block.format(
                        range,
                        if bold { Some(!flags.0) } else { None },
                        if bold { None } else { Some(!flags.1) },
                        None,
                    );
                }
            }
            self.changed(cx);
            return;
        }
        let row = self
            .view
            .portal_list(cx, ids!(blocks))
            .item(cx, self.selected, id!(Text));
        let input = row.article_rich_input(cx, ids!(rich));
        self.history.checkpoint(&self.document);
        if input.toggle_format(cx, bold) {
            if let (Some(block), Some((text, marks))) =
                (self.document.blocks.get_mut(self.selected), input.content())
            {
                block.text = text;
                block.marks = marks;
            }
            self.changed(cx);
        }
    }
}

impl Widget for MarkdownEditor {
    fn script_call(
        &mut self,
        vm: &mut ScriptVm,
        method: LiveId,
        args: ScriptValue,
    ) -> ScriptAsyncResult {
        if method == live_id!(text) {
            return ScriptAsyncResult::Return(
                vm.bx.heap.new_string_from_str(&self.markdown).into(),
            );
        }
        // Unlike generic set_text, load exposes validation failure. A caller
        // must not replace its local draft when a larger remote file fails.
        if method == live_id!(load) {
            let text = args.as_object().and_then(|obj| {
                let trap = vm.bx.threads.cur().trap.pass();
                let value = vm.bx.heap.vec_value(obj, 0, trap);
                vm.bx.heap.cast_to_owned_string(value, "loading Markdown")
            });
            let result = match text {
                Some(text) => vm.with_cx_mut(|cx| self.replace_markdown(cx, &text, false)),
                None => Err("load requires Markdown text".into()),
            };
            return ScriptAsyncResult::Return(
                vm.bx
                    .heap
                    .new_string_from_str(&result.err().unwrap_or_default())
                    .into(),
            );
        }
        ScriptAsyncResult::MethodNotFound
    }
    fn text(&self) -> String {
        self.markdown.clone()
    }
    fn set_text(&mut self, cx: &mut Cx, text: &str) {
        if let Err(error) = self.replace_markdown(cx, text, false) {
            self.parse_error = Some(error);
            self.render(cx);
        }
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if let Some(owner) = cx.script_ref_vm_id(&self.source) {
            if widget_async::current_splash_vm_id(cx) != owner {
                return widget_async::with_isolate(cx, owner, |cx| {
                    self.handle_event(cx, event, scope)
                });
            }
        }
        let list = self.view.portal_list(cx, ids!(blocks));
        if self.mode == Mode::Rich {
            match self.selection.handle_event(
                cx,
                event,
                &list,
                &mut self.document,
                &mut self.history,
            ) {
                SelectionUpdate::Changed => {
                    self.changed(cx);
                    return;
                }
                SelectionUpdate::Handled => return,
                SelectionUpdate::Pass => {}
            }
        }
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        if self.mode == Mode::Rich && matches!(event, Event::MouseDown(_) | Event::MouseUp(_)) {
            self.selection.after_event(cx, &list, &self.document);
        }
        if let Some(text) = self.view.text_input(cx, ids!(markdown)).changed(&actions) {
            // Incomplete source remains the user's authoritative draft even if
            // the rich document cannot parse it. Never silently revert an edit.
            match Document::from_markdown("", &text) {
                Ok(mut doc) => {
                    self.history.checkpoint(&self.document);
                    doc.theme = self.document.theme;
                    self.document = doc;
                    self.parse_error = None;
                }
                Err(error) => self.parse_error = Some(error),
            }
            self.markdown = text;
            self.preview_dirty = true;
            self.emit_change(cx);
            self.render(cx);
        }
        for (index, row) in list.items_with_actions(&actions) {
            self.selected = index;
            let input = row.article_rich_input(cx, ids!(rich));
            if input.changed(&actions).is_some() {
                self.history.checkpoint(&self.document);
                if let (Some(block), Some((text, marks))) =
                    (self.document.blocks.get_mut(index), input.content())
                {
                    block.text = text;
                    block.marks = marks;
                }
                self.changed(cx);
            }
        }
        if self.view.button(cx, ids!(bold)).clicked(&actions) {
            self.format(cx, true);
        }
        if self.view.button(cx, ids!(italic)).clicked(&actions) {
            self.format(cx, false);
        }
        for (path, kind) in [
            (ids!(heading), BlockKind::Heading2),
            (ids!(bullet), BlockKind::Bullet),
            (ids!(quote), BlockKind::Quote),
        ] {
            if self.view.button(cx, path).clicked(&actions) {
                self.history.checkpoint(&self.document);
                if let Some(block) = self.document.blocks.get_mut(self.selected) {
                    block.kind = if block.kind == kind {
                        BlockKind::Paragraph
                    } else {
                        kind
                    };
                }
                self.changed(cx);
            }
        }
        for (path, kind, text) in [
            (ids!(paragraph), BlockKind::Paragraph, ""),
            (
                ids!(table),
                BlockKind::Markdown,
                "| Column | Column |\n| --- | --- |\n| Value | Value |",
            ),
            (ids!(code), BlockKind::Markdown, "```text\nCode here\n```"),
            (ids!(math), BlockKind::Markdown, "$$\nx^2 + y^2 = z^2\n$$"),
            (
                ids!(link),
                BlockKind::Markdown,
                "[Link text](https://example.com)",
            ),
            (
                ids!(image),
                BlockKind::Markdown,
                "![Image description](https://example.com/image.png)",
            ),
        ] {
            if self.view.button(cx, path).clicked(&actions) {
                self.history.checkpoint(&self.document);
                self.selected = (self.selected + 1).min(self.document.blocks.len());
                self.document
                    .blocks
                    .insert(self.selected, Block::new(kind, text));
                self.selection.reset();
                list.set_first_id_and_scroll(self.selected, 0.0);
                self.changed(cx);
            }
        }
        for (path, redo) in [(ids!(undo), false), (ids!(redo), true)] {
            if self.view.button(cx, path).clicked(&actions) {
                let changed = if redo {
                    self.history.redo(&mut self.document)
                } else {
                    self.history.undo(&mut self.document)
                };
                if changed {
                    self.selected = 0;
                    self.selection.reset();
                    self.changed(cx);
                }
            }
        }
        if self.view.button(cx, ids!(theme)).clicked(&actions) {
            let index = Theme::ALL
                .iter()
                .position(|theme| *theme == self.document.theme)
                .unwrap_or(0);
            self.document.theme = Theme::ALL[(index + 1) % Theme::ALL.len()];
            self.preview_dirty = true;
            self.render(cx);
        }
        for (path, mode) in [
            (ids!(rich_mode), Mode::Rich),
            (ids!(source_mode), Mode::Source),
            (ids!(preview_mode), Mode::Preview),
        ] {
            if self.view.button(cx, path).clicked(&actions) {
                if mode == Mode::Rich && self.parse_error.is_some() {
                    self.render(cx);
                    continue;
                }
                self.mode = mode;
                if self.mode == Mode::Source {
                    self.view
                        .text_input(cx, ids!(markdown))
                        .set_text(cx, &self.markdown);
                }
                cx.hide_text_ime();
                cx.set_key_focus(Area::Empty);
                self.render(cx);
            }
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if let Some(owner) = cx.script_ref_vm_id(&self.source) {
            if widget_async::current_splash_vm_id(cx) != owner {
                return widget_async::with_isolate(cx, owner, |cx| self.draw_walk(cx, scope, walk));
            }
        }
        if !self.initialized {
            self.initialized = true;
            self.preview_dirty = true;
            self.render(cx);
        }
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.as_portal_list().borrow_mut() {
                list.set_item_range(cx, 0, self.document.blocks.len());
                while let Some(index) = list.next_visible_item(cx) {
                    let Some(block) = self.document.blocks.get(index) else {
                        continue;
                    };
                    let row = list.item(cx, index, id!(Text));
                    row.label(cx, ids!(kind)).set_text(
                        cx,
                        match block.kind {
                            BlockKind::Markdown | BlockKind::Html => {
                                "Markdown block · exact source"
                            }
                            BlockKind::Bullet => "•",
                            BlockKind::Numbered => "1.",
                            BlockKind::Quote => "Quote",
                            BlockKind::Divider => "Divider",
                            _ => "",
                        },
                    );
                    let input = row.article_rich_input(cx, ids!(rich));
                    presentation::style_input(cx, input.clone(), &self.document, block);
                    input.set_empty_text(
                        cx,
                        if presentation::show_body_placeholder(&self.document, index) {
                            "Write your note…".into()
                        } else {
                            String::new()
                        },
                    );
                    self.selection
                        .apply_to_input(cx, index, &input, &self.document);
                    row.draw_all(cx, scope);
                    self.selection.after_draw(cx, index, &input);
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
    fn imported_markdown_retains_github_bytes_until_body_edit() {
        let source = "# Notes\r\n\r\n- [x] Done\r\n\r\n```rust\r\nlet x = 1;\r\n```\r\n";
        let mut document = Document::from_markdown("", source).unwrap();
        document.theme = Theme::Ocean;
        assert_eq!(document.markdown(), source);
    }
    #[test]
    fn rich_edit_undo_restores_the_original_markdown_snapshot() {
        let source = "Keep **these** words.\n\n## Next\n";
        let mut document = Document::from_markdown("", source).unwrap();
        let mut history = EditHistory::default();
        history.checkpoint(&document);
        document.blocks[0].text = "Changed through the rich editor".into();
        document.blocks[0].marks.clear();
        assert!(document
            .markdown()
            .contains("Changed through the rich editor"));
        assert!(history.undo(&mut document));
        assert_eq!(document.markdown(), source);
        assert!(history.redo(&mut document));
        assert!(document
            .markdown()
            .contains("Changed through the rich editor"));
    }
}
