//! Rinx's article editor without its Matrix account/publication controller.
//!
//! The widget owns only editing state. Its embedding app owns local storage,
//! repository selection and host-reviewed publication. No account, filesystem,
//! network or GitHub credential is reachable through this widget.
mod table_picker;
use table_picker::TableSizePickerWidgetExt;
use makepad_widgets::makepad_draw::text::selection::{Cursor, Selection};
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

#[cfg(target_os = "macos")]
const WRITER_TOP: f64 = 28.0;
#[cfg(not(target_os = "macos"))]
const WRITER_TOP: f64 = 0.0;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    let WriterIcon = Button {
        width: 44 height: 44 margin: 0 padding: 0 spacing: 0
        text: "" grab_key_focus: false enable_long_press: true
        align: Align{x: 0.5 y: 0.5}
        icon_walk: Walk{width: 20 height: 20}
        draw_icon.color: #x242424
        draw_bg +: {color: #xffffff color_hover: #xf5f5f5 color_down: #xe9f8ef color_focus: #xffffff border_size: 0 border_radius: 6}
    }
    let WriterTool = WriterIcon {width: 36 height: 36 draw_bg +: {color: #x00000000 color_hover: #xf5f5f5 color_down: #xe9f8ef}}
    // Per-instance geometry keeps differently sized frames from sharing a
    // rounded-view uniform when the native renderer batches their draw calls.
    let WriterFrame = RoundedView {draw_bg +: {border_radius: instance(4.0) border_size: instance(0.0)}}
    let WriterLabel = Label {padding: 0 draw_text +: {color: #x191919 text_style: theme.font_regular{font_size: 13}}}
    let WriterSwatch = Button {width: 92 height: 72 margin: 0 padding: 6 text: ""
        draw_bg +: {color: #xffffff border_size: 1 border_color: #xe5e5e5 border_radius: 4}
        draw_text +: {color: #x191919 text_style: theme.font_regular{font_size: 10}}
    }
    mod.widgets.MarkdownEditorBase = #(MarkdownEditor::register_widget(vm))
    mod.widgets.MarkdownEditor = set_type_default() do mod.widgets.MarkdownEditorBase {
        width: Fill height: Fill flow: Overlay padding: Inset{top: #(WRITER_TOP)} show_bg: true draw_bg +: {color: #xebebeb}
        icon_tooltip: CalloutTooltip {}
        body := SolidView {width: Fill height: Fill flow: Down draw_bg.color: #xebebeb
            header := View {width: Fill height: 52 flow: Right align: Align{y: 0.5} padding: Inset{left: 8 right: 12} spacing: 8
                repository_button := WriterIcon {width: 36 draw_bg +: {color: #x00000000 color_hover: #x00000000 color_down: #x00000000 color_focus: #x00000000 border_size: 0} icon_walk: Walk{width: 8 height: 14} draw_icon +: {svg: crate_resource("self://resources/icons/chevron_left.svg") color: #x191919}}
                editor_heading := WriterLabel {text: "Notes" draw_text.text_style: theme.font_bold{font_size: 16}}
                title_box := View {width: Fill height: Fill align: Align{x: 0.5 y: 0.5}
                    destination := WriterLabel {width: 300 draw_text.wrap: Ellipsis draw_text.text_style: theme.font_bold{font_size: 15}}
                }
                header_fill := View {width: Fill height: Fill visible: false}
                palette_button := WriterIcon {draw_bg +: {border_size: 1 border_color: #x9fdcb8} draw_icon +: {svg: crate_resource("self://resources/icons/article_palette.svg") color: #x07a858}}
                WriterFrame {width: Fit height: 48 flow: Right padding: 2 spacing: 0 draw_bg +: {color: #xebebeb border_radius: 6}
                    source_mode := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/edit.svg")}}
                    split_mode := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/article_split.svg")}}
                    preview_mode := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/eye_open.svg")}}
                }
                saved := WriterLabel {text: "Saved locally" draw_text +: {color: #x888888 text_style: theme.font_regular{font_size: 11}}}
                save_button := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/send.svg") color: #xffffff} draw_bg +: {color: #x09616f color_hover: #x085763 color_down: #x074d58}}
            }
            panes := View {width: Fill height: Fill flow: Right
                source_pane := SolidView {width: Fill height: Fill flow: Down draw_bg.color: #xebebeb
                    title_small_box := View {visible: false width: Fill height: Fit padding: Inset{left: 12 right: 12 top: 12}
                        WriterFrame {width: Fill height: 48 margin: Inset{top: 3 bottom: 3} padding: 12 align: Align{y: 0.5} draw_bg +: {color: #xebebeb border_size: 0.5 border_color: #xc5c5c5 border_radius: 6}
                            destination_small := WriterLabel {width: Fill draw_text.wrap: Ellipsis draw_text.text_style: theme.font_bold{font_size: 16}}
                        }
                    }
                    formatting := View {width: Fill height: 44 flow: Right align: Align{y: 0.5} padding: Inset{left: 10 right: 10}
                        bold := WriterTool { draw_icon +: {svg: crate_resource("self://resources/icons/article_bold.svg")}}
                        italic := WriterTool { draw_icon +: {svg: crate_resource("self://resources/icons/article_italic.svg")}}
                        heading := WriterTool { draw_icon +: {svg: crate_resource("self://resources/icons/article_heading.svg")}}
                        quote := WriterTool { draw_icon +: {svg: crate_resource("self://resources/icons/article_quote.svg")}}
                        bullet := WriterTool { draw_icon +: {svg: crate_resource("self://resources/icons/article_bullet.svg")}}
                        numbered := WriterTool { draw_icon +: {svg: crate_resource("self://resources/icons/article_numbered.svg")}}
                        link := WriterTool { draw_icon +: {svg: crate_resource("self://resources/icons/link.svg")}}
                        image := WriterTool { draw_icon +: {svg: crate_resource("self://resources/icons/add_photo.svg")}}
                        code := WriterTool { draw_icon +: {svg: crate_resource("self://resources/icons/view_source.svg")}}
                        table := WriterTool { draw_icon +: {svg: crate_resource("self://resources/icons/article_table.svg")}}
                        rule := WriterTool { draw_icon +: {svg: crate_resource("self://resources/icons/article_rule.svg")}}
                    }
                    SolidView {width: Fill height: 1 draw_bg.color: #xe5e5e5}
                    markdown := TextInput {width: Fill height: Fill is_multiline: true flow: Right{wrap: true}
                        empty_text: "Write Markdown here…" padding: Inset{left: 16 right: 16 top: 12 bottom: 12}
                        draw_text +: {color: #x191919 color_hover: #x191919 color_focus: #x191919 color_empty: #x999999 color_empty_hover: #x999999 color_empty_focus: #x999999 text_style: theme.font_regular{font_size: 13 line_spacing: 1.5}}
                        draw_bg +: {color: #xebebeb color_hover: #xebebeb color_focus: #xebebeb color_down: #xebebeb color_empty: #xebebeb color_2: #xebebeb color_2_hover: #xebebeb color_2_focus: #xebebeb color_2_down: #xebebeb color_2_empty: #xebebeb border_size: 0 border_radius: 0}
                        draw_cursor.color: #x07c160
                        draw_selection +: {color: #x9ab3d440 color_focus: #x9ab3d440}
                    }
                    bottom := View {visible: false width: Fill height: Fit flow: Down padding: Inset{left: 12 right: 12 bottom: 8} spacing: 6
                        saved_small := WriterLabel {draw_text +: {color: #x888888 text_style: theme.font_regular{font_size: 11}}}
                        WriterFrame {width: Fill height: 44 flow: Right align: Align{y: 0.5} padding: Inset{left: 6 right: 6} draw_bg +: {color: #xebebeb border_size: 1 border_color: #xc5c5c5 border_radius: 6}
                            mobile_bold := WriterTool {width: Fill  draw_icon +: {svg: crate_resource("self://resources/icons/article_bold.svg")}}
                            mobile_italic := WriterTool {width: Fill  draw_icon +: {svg: crate_resource("self://resources/icons/article_italic.svg")}}
                            mobile_heading := WriterTool {width: Fill  draw_icon +: {svg: crate_resource("self://resources/icons/article_heading.svg")}}
                            mobile_quote := WriterTool {width: Fill  draw_icon +: {svg: crate_resource("self://resources/icons/article_quote.svg")}}
                            mobile_bullet := WriterTool {width: Fill  draw_icon +: {svg: crate_resource("self://resources/icons/article_bullet.svg")}}
                            mobile_table := WriterTool {width: Fill  draw_icon +: {svg: crate_resource("self://resources/icons/article_table.svg")}}
                            mobile_image := WriterTool {width: Fill  draw_icon +: {svg: crate_resource("self://resources/icons/add_photo.svg")}}
                            mobile_link := WriterTool {width: Fill  draw_icon +: {svg: crate_resource("self://resources/icons/link.svg")}}
                        }
                    }
                }
                divider := SolidView {width: 1 height: Fill draw_bg.color: #xe5e5e5}
                preview_pane := SolidView {width: Fill height: Fill flow: Down align: Align{x: 0.5} padding: Inset{top: 16 bottom: 16} draw_bg.color: #xebebeb
                    paper := WriterFrame {width: 420 height: Fill flow: Down padding: Inset{left: 24 right: 24 top: 24 bottom: 8} draw_bg +: {color: #xffffff border_size: 1 border_color: #xe8e8e8 border_radius: 4}
                        preview_list := PortalList {width: Fill height: Fill drag_scrolling: true
                            PreviewBlock := View {width: Fill height: Fit flow: Down padding: Inset{bottom: 12}
                                preview := Html {width: Fill height: Fit padding: 0 selectable: true
                                    font_size: 14 font_color: #x191919 draw_text.color: #x191919
                                    text_style_normal: theme.font_regular{font_size: 14}
                                    text_style_bold: theme.font_bold{font_size: 14}
                                    text_style_italic: theme.font_italic{font_size: 14}
                                    text_style_bold_italic: theme.font_bold_italic{font_size: 14}
                                    table_walk: Walk{width: Fill height: Fit}
                                    table_layout: Layout{flow: Down}
                                    table_row_walk: Walk{width: Fill height: Fit}
                                    table_row_layout: Layout{flow: Right}
                                    table_cell_layout: Layout{flow: Right{wrap: true} padding: Inset{left: 6 right: 6 top: 4 bottom: 4}}
                                    rmath := ArticleMath {} rdiagram := ArticleDiagram {}
                                    rimage := ArticleImage {} remoji := ArticleEmoji {}
                                    rcode := ArticleCode {} rcell := ArticleCell {}
                                }
                            }
                        }
                    }
                }
        rich_pane := View {width: Fill height: Fill visible: false
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
                        // The theme's caret is white: on these white blocks it
                        // blinked unseen. The source view's green, here too.
                        draw_cursor.color: #x07c160
                        draw_selection +: {color: #x9ab3d440 color_focus: #x9ab3d440}
                    }
                }
            }
        }

                styles_panel := SolidView {visible: false width: 280 height: Fill flow: Down padding: 16 spacing: 10 draw_bg.color: #xffffff
                    View {width: Fill height: 44 align: Align{y: 0.5}
                        WriterLabel {width: Fill text: "Article style" draw_text.text_style: theme.font_bold{font_size: 14}}
                        styles_close := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/close.svg")}}
                    }
                    ScrollYView {width: Fill height: Fill flow: Down spacing: 10
                        View {width: Fill height: Fit spacing: 10 theme_0 := WriterSwatch {width: Fill} theme_1 := WriterSwatch {width: Fill}}
                        View {width: Fill height: Fit spacing: 10 theme_2 := WriterSwatch {width: Fill} theme_3 := WriterSwatch {width: Fill}}
                        View {width: Fill height: Fit spacing: 10 theme_4 := WriterSwatch {width: Fill} theme_5 := WriterSwatch {width: Fill}}
                        View {width: Fill height: Fit spacing: 10 theme_6 := WriterSwatch {width: Fill} theme_7 := WriterSwatch {width: Fill}}
                        View {width: Fill height: Fit spacing: 10 theme_8 := WriterSwatch {width: Fill} theme_9 := WriterSwatch {width: Fill}}
                        View {width: Fill height: Fit spacing: 10 theme_10 := WriterSwatch {width: Fill} theme_11 := WriterSwatch {width: Fill}}
                        rich_mode := Button {width: Fill height: 44 text: "Block editor" grab_key_focus: false}
                        View {width: Fill height: 44 align: Align{x: 0.5}
                            undo_button := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/article_undo.svg")}}
                            redo_button := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/article_redo.svg")}}
                        }
                    }
                }
            }
            stats_box := View {width: Fill height: Fit flow: Down
                SolidView {width: Fill height: 1 draw_bg.color: #xc5c5c5}
                stats := WriterLabel {width: Fill padding: Inset{left: 16 top: 8 bottom: 8} draw_text +: {color: #x888888 text_style: theme.font_regular{font_size: 11}}}
            }
            status_box := View {width: Fill height: Fit visible: false padding: Inset{left: 16 right: 16 top: 6 bottom: 8}
                editor_status := WriterLabel {width: Fill draw_text.wrap: Words draw_text +: {color: #xc05a00 text_style: theme.font_regular{font_size: 11}}}
            }
        }
        style_fab_box := View {visible: false width: Fill height: Fill align: Align{x: 1 y: 1} padding: Inset{right: 16 bottom: 64}
            style_fab := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/article_palette.svg") color: #x07a858} draw_bg +: {border_size: 1 border_color: #x9fdcb8 border_radius: 22}}
        }
        style_sheet := SolidView {visible: false width: Fill height: Fill flow: Down align: Align{y: 1} draw_bg.color: #x00000055
            WriterFrame {width: Fill height: Fit flow: Down padding: Inset{left: 16 right: 16 top: 8 bottom: 20} spacing: 12 draw_bg +: {color: #xffffff border_radius: 12}
                View {width: Fill height: Fit align: Align{x: 0.5} WriterFrame {width: 36 height: 4 draw_bg +: {color: #xdddddd border_radius: 2}}}
                View {width: Fill height: 44 align: Align{y: 0.5}
                    WriterLabel {width: Fill text: "Article style" draw_text.text_style: theme.font_bold{font_size: 16}}
                    styles_cancel := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/close.svg")}}
                }
                ScrollXView {width: Fill height: 92 flow: Right spacing: 10
                    mobile_theme_0 := WriterSwatch {}
                    mobile_theme_1 := WriterSwatch {}
                    mobile_theme_2 := WriterSwatch {}
                    mobile_theme_3 := WriterSwatch {}
                    mobile_theme_4 := WriterSwatch {}
                    mobile_theme_5 := WriterSwatch {}
                    mobile_theme_6 := WriterSwatch {}
                    mobile_theme_7 := WriterSwatch {}
                    mobile_theme_8 := WriterSwatch {}
                    mobile_theme_9 := WriterSwatch {}
                    mobile_theme_10 := WriterSwatch {}
                    mobile_theme_11 := WriterSwatch {}
                }
                View {width: Fill height: 44 align: Align{y: 0.5}
                    mobile_rich_mode := Button {width: Fill height: 44 text: "Block editor" grab_key_focus: false}
                    mobile_undo_button := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/article_undo.svg")}}
                    mobile_redo_button := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/article_redo.svg")}}
                }
                styles_apply := WriterIcon {width: Fill draw_icon +: {svg: crate_resource("self://resources/icons/checkmark.svg") color: #xffffff} draw_bg +: {color: #x09616f color_hover: #x085763 color_down: #x074d58}}
            }
        }
        table_popup := SolidView {visible: false width: Fill height: Fill flow: Down align: Align{x: 0.5 y: 0.5} draw_bg.color: #x00000033
            WriterFrame {width: Fit height: Fit flow: Down padding: 14 spacing: 10 draw_bg +: {color: #xffffff border_size: 1 border_color: #xdddddd border_radius: 8}
                View {width: Fill height: 44 align: Align{y: 0.5}
                    WriterLabel {width: Fill text: "Insert table" draw_text.text_style: theme.font_bold{font_size: 13}}
                    table_close := WriterIcon {draw_icon +: {svg: crate_resource("self://resources/icons/close.svg")}}
                }
                table_picker := TableSizePicker {}
            }
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
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    article_makepad::apple_fonts::install(vm);
    article_makepad::script_mod(vm);
    table_picker::script_mod(vm);
    script_mod(vm);
    // Makepad's public prelude is a snapshot assembled before host mods.
    // Export only the safe composite, not Rinx's internal widget vocabulary.
    script_eval!(vm, {mod.prelude.widgets.MarkdownEditor = mod.widgets.MarkdownEditor});
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Mode {
    Rich,
    Source,
    #[default]
    Split,
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
    #[live]
    on_repository: Option<ScriptFnRef>,
    #[live]
    on_save: Option<ScriptFnRef>,
    #[live]
    icon_tooltip: CalloutTooltip,
    #[rust]
    icon_hint: Option<WidgetUid>,
    #[rust]
    wide: bool,
    #[rust]
    styles_open: bool,
    #[rust]
    style_before: Option<Theme>,
    #[rust]
    table_open: bool,
    #[rust]
    destination: String,
    #[rust]
    status: String,
    #[rust]
    undo_markdown: Vec<String>,
    #[rust]
    redo_markdown: Vec<String>,
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
    #[rust]
    preview_blocks: Vec<article_core::markdown_render::RenderedBlock>,
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
            self.undo_markdown.clear();
            self.redo_markdown.clear();
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

    fn emit_callback(&self, cx: &mut Cx, handler: Option<ScriptFnRef>) {
        if let Some(handler) = handler {
            cx.widget_to_script_call(self.widget_uid(), NIL, self.source.clone(), handler, &[]);
        }
    }

    fn effective_mode(&self) -> Mode {
        if self.mode == Mode::Split && !self.wide { Mode::Source } else { self.mode }
    }

    fn render(&mut self, cx: &mut Cx) {
        let mode = self.effective_mode();
        let source = matches!(mode, Mode::Source | Mode::Split);
        let preview = matches!(mode, Mode::Preview | Mode::Split);
        for (path, visible) in [
            (ids!(rich_pane), mode == Mode::Rich),
            (ids!(source_pane), source),
            (ids!(preview_pane), preview),
            (ids!(divider), source && preview),
            (ids!(formatting), self.wide),
            (ids!(editor_heading), self.wide),
            (ids!(title_box), self.wide),
            (ids!(header_fill), !self.wide),
            (ids!(title_small_box), !self.wide),
            (ids!(palette_button), self.wide),
            (ids!(saved), self.wide),
            (ids!(split_mode), self.wide),
            (ids!(bottom), !self.wide),
            (ids!(styles_panel), self.styles_open && self.wide),
            (ids!(style_sheet), self.styles_open && !self.wide),
            (ids!(style_fab_box), !self.wide && mode == Mode::Preview && !self.styles_open),
            (ids!(table_popup), self.table_open),
            (ids!(stats_box), self.wide || mode == Mode::Preview),
        ] {
            self.view.widget(cx, path).set_visible(cx, visible);
        }
        for (path, selected) in [
            (ids!(source_mode), mode == Mode::Source || mode == Mode::Rich),
            (ids!(split_mode), mode == Mode::Split),
            (ids!(preview_mode), mode == Mode::Preview),
        ] {
            let mut button = self.view.button(cx, path);
            let bg = color(if selected { 0xe9f8ef } else { 0xffffff });
            script_apply_eval!(cx, button, {draw_bg +: {color: #(bg) border_radius: 6 border_size: 0}});
            if let Some(mut button) = button.borrow_mut() {
                button.draw_icon.color = color(if selected { 0x07a858 } else { 0x555555 });
            };
        }
        let width = if self.wide { Size::Fixed(420.0) } else { Size::fill() };
        let paper_color = color(self.document.theme.colors().0);
        let mut paper = self.view.view(cx, ids!(paper));
        script_apply_eval!(cx, paper, {width: #(width) draw_bg +: {color: #(paper_color) border_size: 1 border_color: #xe8e8e8 border_radius: 4}});
        if preview && self.preview_dirty {
            let images = article_makepad::content::Images::default();
            let mut renderer = article_makepad::content::NativeRenderer {
                images: &images, size: 14.0, ink: self.document.theme.colors().1,
            };
            self.preview_blocks = article_core::markdown_render::render(&self.markdown, &mut renderer);
            self.preview_dirty = false;
        }
        for path in [ids!(destination), ids!(destination_small)] {
            self.view.label(cx, path).set_text(cx, if self.destination.is_empty() { "Untitled note" } else { &self.destination });
        }
        // Long provider/error messages remain readable; they never widen the icon header.
        let status = if self.status.is_empty() { "Saved locally" } else { &self.status };
        let compact = if status.chars().count() > 28 { "See status below" } else { status };
        self.view.label(cx, ids!(saved)).set_text(cx, compact);
        self.view.label(cx, ids!(saved_small)).set_text(cx, compact);
        let (characters, minutes, images) = self.document.stats();
        self.view.label(cx, ids!(stats)).set_text(cx, &format!("{characters} chars · about {minutes} min · {images} images"));
        let details = self.parse_error.as_deref().unwrap_or(if status.chars().count() > 28 { status } else { "" });
        self.view.widget(cx, ids!(status_box)).set_visible(cx, !details.is_empty());
        self.view.label(cx, ids!(editor_status)).set_text(cx, details);
        if self.styles_open {
            for (index, theme) in Theme::ALL.iter().enumerate() {
                for prefix in ["theme_", "mobile_theme_"] {
                    let mut button = self.view.button(cx, &[LiveId::from_str(&format!("{prefix}{index}"))]);
                    let (paper, ink, accent) = theme.colors();
                    let (paper, ink, border) = (color(paper), color(ink), color(if *theme == self.document.theme { accent } else { 0xe5e5e5 }));
                    button.set_text(cx, theme.name());
                    script_apply_eval!(cx, button, {draw_bg +: {color: #(paper) border_color: #(border)} draw_text +: {color: #(ink)}});
                }
            }
        }
        self.view.redraw(cx);
    }

    fn checkpoint_text(&mut self) {
        if self.undo_markdown.last() != Some(&self.markdown) {
            if self.undo_markdown.len() == 100 { self.undo_markdown.remove(0); }
            self.undo_markdown.push(self.markdown.clone());
        }
        self.redo_markdown.clear();
    }

    fn source_changed(&mut self, cx: &mut Cx, text: String) {
        if text != self.markdown { self.checkpoint_text(); }
        self.apply_source_change(cx, text);
    }

    fn apply_source_change(&mut self, cx: &mut Cx, text: String) {
        match Document::from_markdown("", &text) {
            Ok(mut document) => {
                document.theme = self.document.theme;
                document.large_type = self.document.large_type;
                document.compact = self.document.compact;
                self.document = document;
                self.parse_error = None;
            }
            Err(error) => self.parse_error = Some(error),
        }
        self.markdown = text;
        self.preview_dirty = true;
        self.emit_change(cx);
        self.render(cx);
    }

    // These selection operations follow Rinx's writer. They edit only the selected
    // source bytes, preserving file whitespace and headers outside the selection.
    fn wrap_source(&mut self, cx: &mut Cx, before: &str, after: &str, placeholder: &str) {
        let input = self.view.text_input(cx, ids!(markdown));
        let selection = input.selection();
        let (start, end) = (selection.start().index, selection.end().index);
        let inner = if start == end { placeholder.to_owned() } else { input.selected_text() };
        let _ = input.replace_range(cx, start..end, &format!("{before}{inner}{after}"), UndoGroup::New);
        input.set_selection(cx, Selection {
            anchor: Cursor { index: start + before.len(), prefer_next_row: false },
            cursor: Cursor { index: start + before.len() + inner.len(), prefer_next_row: false },
        });
        self.source_changed(cx, input.text());
        input.set_key_focus(cx);
    }

    fn prefix_source(&mut self, cx: &mut Cx, prefix: impl Fn(usize) -> String) {
        let input = self.view.text_input(cx, ids!(markdown));
        let text = input.text();
        let selection = input.selection();
        let start = text[..selection.start().index].rfind('\n').map_or(0, |i| i + 1);
        let end = text[selection.end().index..].find('\n').map_or(text.len(), |i| selection.end().index + i);
        let replacement = text[start..end].split('\n').enumerate().map(|(i, line)| format!("{}{line}", prefix(i))).collect::<Vec<_>>().join("\n");
        let _ = input.replace_range(cx, start..end, &replacement, UndoGroup::New);
        let caret = Cursor { index: start + replacement.len(), prefer_next_row: false };
        input.set_selection(cx, Selection { anchor: caret, cursor: caret });
        self.source_changed(cx, input.text());
        input.set_key_focus(cx);
    }

    fn insert_source(&mut self, cx: &mut Cx, text: &str) {
        let input = self.view.text_input(cx, ids!(markdown));
        let selection = input.selection();
        let start = selection.start().index;
        let _ = input.replace_range(cx, start..selection.end().index, text, UndoGroup::New);
        let caret = Cursor { index: start + text.len(), prefer_next_row: false };
        input.set_selection(cx, Selection { anchor: caret, cursor: caret });
        self.source_changed(cx, input.text());
        input.set_key_focus(cx);
    }

    fn icon_hints(&mut self, cx: &mut Cx, event: &Event) {
        self.icon_tooltip.handle_event(cx, event, &mut Scope::empty());
        let mut hint = None;
        for (id, label) in [
            (id!(repository_button), "Choose repository or file"), (id!(save_button), "Review and save to GitHub"),
            (id!(source_mode), "Edit Markdown"), (id!(split_mode), "Split editor and preview"),
            (id!(preview_mode), "Preview"), (id!(palette_button), "Article style"), (id!(style_fab), "Article style"),
            (id!(styles_close), "Close styles"), (id!(styles_cancel), "Cancel style change"),
            (id!(styles_apply), "Apply style"), (id!(table_close), "Cancel table"),
            (id!(bold), "Bold"), (id!(mobile_bold), "Bold"), (id!(italic), "Italic"), (id!(mobile_italic), "Italic"),
            (id!(heading), "Heading"), (id!(mobile_heading), "Heading"), (id!(quote), "Quote"), (id!(mobile_quote), "Quote"),
            (id!(bullet), "Bulleted list"), (id!(mobile_bullet), "Bulleted list"), (id!(numbered), "Numbered list"),
            (id!(link), "Link"), (id!(mobile_link), "Link"), (id!(image), "Image URL"), (id!(mobile_image), "Image URL"),
            (id!(code), "Code"), (id!(table), "Insert table"), (id!(mobile_table), "Insert table"), (id!(rule), "Horizontal rule"),
            (id!(undo_button), "Undo"), (id!(redo_button), "Redo"), (id!(mobile_undo_button), "Undo"), (id!(mobile_redo_button), "Redo"),
        ] {
            let button = self.view.button(cx, &[id]);
            let area = button.area();
            if !button.visible() || !area.is_valid(cx) { continue; }
            let targeted = match event {
                Event::MouseMove(e) => e.handled.get() == area && area.clipped_rect(cx).contains(e.abs),
                Event::LongPress(e) => cx.fingers.touch_capture_area(e.uid) == Some(area),
                _ => false,
            };
            if targeted { hint = Some((button.widget_uid(), label, area.rect(cx))); break; }
        }
        if let Some((uid, label, rect)) = hint {
            if self.icon_hint != Some(uid) {
                self.icon_tooltip.show_with_options(cx, label, rect, CalloutTooltipOptions {position: TooltipPosition::Top, ..Default::default()}, false);
                self.icon_hint = Some(uid);
            }
        } else if self.icon_hint.take().is_some() { self.icon_tooltip.hide(cx); }
    }

    fn changed(&mut self, cx: &mut Cx) {
        self.checkpoint_text();
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
        if method == live_id!(set_destination) || method == live_id!(set_status) {
            if let Some(text) = args.as_object().and_then(|obj| {
                let trap = vm.bx.threads.cur().trap.pass();
                let value = vm.bx.heap.vec_value(obj, 0, trap);
                vm.bx.heap.cast_to_owned_string(value, "setting writer caption")
            }) {
                if method == live_id!(set_destination) { self.destination = text; } else { self.status = text; }
                vm.with_cx_mut(|cx| self.render(cx));
            }
            return ScriptAsyncResult::Return(NIL);
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
        let actions = cx.capture_actions(|cx| {
            if self.table_open {
                self.view.widget(cx, ids!(table_popup)).handle_event(cx, event, scope);
            } else if self.styles_open && !self.wide {
                self.view.widget(cx, ids!(style_sheet)).handle_event(cx, event, scope);
            } else {
                self.view.handle_event(cx, event, scope);
            }
        });
        if self.mode == Mode::Rich && matches!(event, Event::MouseDown(_) | Event::MouseUp(_)) {
            self.selection.after_event(cx, &list, &self.document);
        }
        // A drag across blocks leaves key focus in the block where it began, so
        // the caret would blink at that block's end of the selection. The block
        // the drag ended in takes focus: the caret blinks where the drag
        // stopped, as Rinx's keyboard extension already moves it.
        if self.mode == Mode::Rich && matches!(event, Event::MouseUp(_)) {
            if let Some(selection) = self.selection.selection {
                if selection.anchor.block != selection.cursor.block {
                    if let Some((_, row)) = list.get_item(selection.cursor.block) {
                        let input = row.article_rich_input(cx, ids!(rich));
                        if !input.is_empty() && !input.has_focus(cx) {
                            input.take_key_focus(cx);
                        }
                    }
                }
            }
        }
        if let Some(text) = self.view.text_input(cx, ids!(markdown)).changed(&actions) {
            self.source_changed(cx, text);
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
        if self.mode == Mode::Rich {
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
        }
        for (desktop, mobile, redo) in [(id!(undo_button), id!(mobile_undo_button), false), (id!(redo_button), id!(mobile_redo_button), true)] {
            if self.view.button(cx, &[desktop]).clicked(&actions) || self.view.button(cx, &[mobile]).clicked(&actions) {
                let previous = if redo { self.redo_markdown.pop() } else { self.undo_markdown.pop() };
                if let Some(previous) = previous {
                    if redo { self.undo_markdown.push(self.markdown.clone()); } else { self.redo_markdown.push(self.markdown.clone()); }
                    self.view.text_input(cx, ids!(markdown)).set_text(cx, &previous);
                    self.selection.reset();
                    self.selected = 0;
                    self.apply_source_change(cx, previous);
                }
            }
        }
        if self.view.button(cx, ids!(repository_button)).clicked(&actions) {
            self.emit_callback(cx, self.on_repository.clone());
        }
        if self.view.button(cx, ids!(save_button)).clicked(&actions) {
            self.emit_callback(cx, self.on_save.clone());
        }
        if self.view.button(cx, ids!(palette_button)).clicked(&actions) || self.view.button(cx, ids!(style_fab)).clicked(&actions) {
            self.styles_open = !self.styles_open;
            self.style_before = self.styles_open.then_some(self.document.theme);
            self.render(cx);
        }
        if self.view.button(cx, ids!(styles_cancel)).clicked(&actions) {
            if let Some(before) = self.style_before.take() { self.document.theme = before; self.preview_dirty = true; }
            self.styles_open = false; self.render(cx);
        }
        if self.view.button(cx, ids!(styles_close)).clicked(&actions) || self.view.button(cx, ids!(styles_apply)).clicked(&actions) {
            self.styles_open = false; self.style_before = None; self.render(cx);
        }
        for (index, theme) in Theme::ALL.iter().enumerate() {
            if ["theme_", "mobile_theme_"].iter().any(|prefix| self.view.button(cx, &[LiveId::from_str(&format!("{prefix}{index}"))]).clicked(&actions)) {
                self.document.theme = *theme; self.preview_dirty = true; self.render(cx);
            }
        }
        if self.mode != Mode::Rich {
            for (id, before, after, placeholder) in [
                (id!(bold), "**", "**", "bold text"), (id!(mobile_bold), "**", "**", "bold text"),
                (id!(italic), "*", "*", "italic text"), (id!(mobile_italic), "*", "*", "italic text"),
                (id!(code), "`", "`", "code"), (id!(link), "[", "](https://)", "link text"), (id!(mobile_link), "[", "](https://)", "link text"),
                (id!(image), "![", "](https://)", "Image description"), (id!(mobile_image), "![", "](https://)", "Image description"),
            ] {
                if self.view.button(cx, &[id]).clicked(&actions) { self.wrap_source(cx, before, after, placeholder); }
            }
            for (desktop, mobile, prefix) in [(id!(heading), id!(mobile_heading), "## "), (id!(quote), id!(mobile_quote), "> "), (id!(bullet), id!(mobile_bullet), "- ")] {
                if self.view.button(cx, &[desktop]).clicked(&actions) || self.view.button(cx, &[mobile]).clicked(&actions) { self.prefix_source(cx, |_| prefix.into()); }
            }
            if self.view.button(cx, ids!(numbered)).clicked(&actions) { self.prefix_source(cx, |i| format!("{}. ", i+1)); }
            if self.view.button(cx, ids!(rule)).clicked(&actions) { self.insert_source(cx, "\n\n---\n\n"); }
            if self.view.button(cx, ids!(table)).clicked(&actions) || self.view.button(cx, ids!(mobile_table)).clicked(&actions) {
                self.table_open = true;
                self.view.table_size_picker(cx, ids!(table_picker)).reset(cx);
                cx.hide_text_ime(); self.render(cx);
            }
        }
        if self.view.button(cx, ids!(table_close)).clicked(&actions) { self.table_open = false; self.render(cx); }
        if let Some((rows, cols)) = self.view.table_size_picker(cx, ids!(table_picker)).picked(&actions) {
            self.table_open = false;
            self.insert_source(cx, &format!("\n\n{}\n\n", table_picker::markdown_table(rows, cols, |i| format!("Column {i}"))));
        }

        for (path, mode) in [
            (ids!(rich_mode), Mode::Rich),
            (ids!(mobile_rich_mode), Mode::Rich),
            (ids!(source_mode), Mode::Source),
            (ids!(split_mode), Mode::Split),
            (ids!(preview_mode), Mode::Preview),
        ] {
            if self.view.button(cx, path).clicked(&actions) {
                if mode == Mode::Rich && self.parse_error.is_some() {
                    self.render(cx);
                    continue;
                }
                self.mode = mode;
                self.styles_open = false;
                if matches!(self.mode, Mode::Source | Mode::Split) {
                    self.view
                        .text_input(cx, ids!(markdown))
                        .set_text(cx, &self.markdown);
                }
                cx.hide_text_ime();
                cx.set_key_focus(Area::Empty);
                self.render(cx);
            }
        }
        self.icon_hints(cx, event);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if let Some(owner) = cx.script_ref_vm_id(&self.source) {
            if widget_async::current_splash_vm_id(cx) != owner {
                return widget_async::with_isolate(cx, owner, |cx| self.draw_walk(cx, scope, walk));
            }
        }
        let available = cx.turtle().inner_rect().size.x;
        let wide = available >= 960.0;
        if self.wide != wide { self.wide = wide; self.render(cx); }
        if !self.initialized {
            self.initialized = true;
            self.preview_dirty = true;
            self.render(cx);
        }
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            let is_preview = step.widget_uid() == self.view.portal_list(cx, ids!(preview_list)).widget_uid();
            if let Some(mut list) = step.as_portal_list().borrow_mut() {
                if is_preview {
                    list.set_item_range(cx, 0, self.preview_blocks.len());
                    while let Some(index) = list.next_visible_item(cx) {
                        let Some(block) = self.preview_blocks.get(index) else { continue; };
                        let row = list.item(cx, index, id!(PreviewBlock));
                        let mut html = row.html(cx, ids!(preview));
                        presentation::style_html(cx, html.clone(), &self.document);
                        html.set_text(cx, &article_makepad::content::native_html(&block.html));
                        row.draw_all(cx, scope);
                    }
                    continue;
                }
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
        self.icon_tooltip.draw_all(cx, &mut Scope::empty());
        DrawStep::done()
    }
}

fn color(rgb: u32) -> Vec4 {
    vec4(((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0, 1.0)
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
