//! A word processor's page for a text draft.
//!
//! `DocumentEditor` shows a draft as it will print: a heading as a heading, bold
//! as bold, a list as a list. Formatting acts on the selection, and typing `# ` or
//! `**word**` makes the style, as a Mac's editors do. Underneath the draft stays the
//! plain text `draft` describes, so what the embedding app stores and converts
//! doesn't change, and a line nobody touched goes back byte for byte.
//!
//! Each line is one block of Rinx's rich input (the same parts as `MarkdownEditor`:
//! native text input, IME, selection across blocks), one a row of a list so a long
//! document stays cheap. A row's template gives it the look of its style.
//!
//! It grants nothing: no file, no account, no network. Script reads and sets its
//! text and gets `on_change` and `on_format`.
//!
//! `DocumentScroller` and `DocMark` are the same document's read-only side: a page
//! that can scroll to a marked heading and say which one it has reached.
use crate::draft::{self, Style};
use article_core::{
    document::{Block, BlockKind, Document, Mark},
    editing::EditHistory,
    selection::{BodySelection, Position},
};
use article_makepad::{
    body_selection::{ArticleSelection, SelectionUpdate},
    rich_input::{ArticleRichInputRef, ArticleRichInputWidgetRefExt},
};
use makepad_widgets::makepad_draw::text::selection::{Cursor, Selection};
use makepad_widgets::makepad_script::ScriptFnRef;
use makepad_widgets::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    // The page stays white with dark ink in both appearances, as a printed page does.
    let doc_ink = #x1c1c1e
    let doc_blue = #x0f4761
    let doc_quote = #x404040
    let doc_hint = #x6e6e73
    let doc_selection = #x0071e340
    let DocInput = ArticleRichInput{
        width: Fill height: Fit is_multiline: true flow: Right{wrap: true} margin: 0 padding: 0
        draw_bg +: {color: #x00000000 color_hover: #x00000000 color_focus: #x00000000 color_down: #x00000000 color_empty: #x00000000 border_size: 0.0 border_radius: 0.0
            border_color: #x00000000 border_color_hover: #x00000000 border_color_focus: #x00000000 border_color_down: #x00000000 border_color_empty: #x00000000
            border_color_2: #x00000000 border_color_2_hover: #x00000000 border_color_2_focus: #x00000000 border_color_2_down: #x00000000 border_color_2_empty: #x00000000}
        draw_text +: {color: doc_ink color_hover: doc_ink color_focus: doc_ink color_down: doc_ink
            color_empty: doc_hint color_empty_hover: doc_hint color_empty_focus: doc_hint
            text_style: theme.font_regular{font_size: 9 line_spacing: 1.3}}
        draw_bold +: {text_style: theme.font_bold{font_size: 9 line_spacing: 1.3}}
        draw_italic +: {text_style: theme.font_italic{font_size: 9 line_spacing: 1.3}}
        draw_bold_italic +: {text_style: theme.font_bold_italic{font_size: 9 line_spacing: 1.3}}
        draw_cursor +: {color: doc_ink}
        draw_selection +: {border_radius: 0.0 color: doc_selection color_hover: doc_selection color_focus: doc_selection color_down: doc_selection color_empty: doc_selection}
    }
    let DocHeading = DocInput{
        draw_text +: {color: doc_blue color_hover: doc_blue color_focus: doc_blue color_down: doc_blue
            text_style: theme.font_regular{font_size: 9 line_spacing: 1.25}}
        draw_bold +: {text_style: theme.font_bold{font_size: 9 line_spacing: 1.25}}
        draw_italic +: {text_style: theme.font_italic{font_size: 9 line_spacing: 1.25}}
        draw_bold_italic +: {text_style: theme.font_bold_italic{font_size: 9 line_spacing: 1.25}}
    }
    let DocRow = View{width: Fill height: Fit flow: Down}

    mod.widgets.DocumentEditorBase = #(DocumentEditor::register_widget(vm))
    mod.widgets.DocumentEditor = set_type_default() do mod.widgets.DocumentEditorBase {
        width: Fill height: Fill flow: Overlay clip_x: false clip_y: false
        empty_text: "Start writing…"
        measure +: {text_style: theme.font_italic{font_size: 9 line_spacing: 1.3}}
        // The sheet the lines lie on, which the app gives its look: it starts `page_gap`
        // down and its top edge goes up with the first line, as a page's does.
        paper := View{width: Fill height: Fill}
        blocks := PortalList{width: Fill height: Fill drag_scrolling: false
            scroll_bar +: {draw_bg +: {color_hover: #x00000059 color_drag: #x00000080}}
            Body := DocRow{rich := DocInput{}}
            H1 := DocRow{rich := DocHeading{}}
            H2 := DocRow{rich := DocHeading{}}
            H3 := DocRow{rich := DocHeading{}}
            // As the document's list style: the mark, then the text a cell along.
            Item := View{width: Fill height: Fit flow: Right
                mark := View{width: 18 height: Fit flow: Overlay
                    dot := RoundedView{width: 5 height: 5 margin: Inset{top: 5} show_bg: true draw_bg +: {color: doc_ink border_radius: 1.25}}
                    num := Label{width: Fit padding: 0 clip_y: false text: "" draw_text +: {color: doc_ink text_style: theme.font_regular{font_size: 9 line_spacing: 1.3}}}
                }
                rich := DocInput{}
            }
            Quote := DocRow{rich := DocInput{
                draw_text +: {color: doc_quote color_hover: doc_quote color_focus: doc_quote color_down: doc_quote
                    text_style: theme.font_italic{font_size: 9 line_spacing: 1.3}}
                draw_bold +: {text_style: theme.font_bold_italic{font_size: 9 line_spacing: 1.3}}
            }}
            // A table's rows and a code fence's lines stay as typed, in the band the document draws code on.
            Code := DocRow{
                band := SolidView{width: Fill height: Fit new_batch: true padding: Inset{left: 10 right: 10 top: 2 bottom: 2} draw_bg.color: #xf2f2f5
                    rich := DocInput{
                        draw_text +: {text_style: theme.font_code{font_size: 8.25}}
                        draw_bold +: {text_style: theme.font_code{font_size: 8.25}}
                        draw_italic +: {text_style: theme.font_code{font_size: 8.25}}
                        draw_bold_italic +: {text_style: theme.font_code{font_size: 8.25}}
                    }
                }
            }
        }
    }

    mod.widgets.DocumentScrollerBase = #(DocumentScroller::register_widget(vm))
    mod.widgets.DocumentScroller = set_type_default() do mod.widgets.DocumentScrollerBase {
        width: Fill height: Fill flow: Down
        scroll_bars: mod.widgets.ScrollBars{show_scroll_x: false show_scroll_y: true scroll_bar_y.drag_scrolling: true}
    }
    mod.widgets.DocMarkBase = #(DocMark::register_widget(vm))
    mod.widgets.DocMark = set_type_default() do mod.widgets.DocMarkBase {
        width: Fill height: Fit flow: Down
    }
}

/// What a block is, beyond its text and marks.
struct Meta {
    style: Style,
    marker: String,
    source: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Typing,
    Deleting,
    Other,
}

struct Snapshot {
    text: String,
    anchor: Position,
    caret: Position,
}

/// B or I pressed with nothing selected: what the next typing is.
#[derive(Clone, Copy, Default)]
struct Pending {
    bold: Option<bool>,
    italic: Option<bool>,
}

/// A key pressed before the row it's for was on the page.
enum Held {
    Key(KeyEvent),
    Text(TextInputEvent),
}

fn template_of(style: Style) -> LiveId {
    match style {
        Style::Body => id!(Body),
        Style::H1 => id!(H1),
        Style::H2 => id!(H2),
        Style::H3 => id!(H3),
        Style::Bullet | Style::Number => id!(Item),
        Style::Quote => id!(Quote),
        Style::Code => id!(Code),
    }
}

/// The document's own sizes (font_size is three quarters of a point): body 12
/// points, headings 20, 16 and 14, code 11.
fn font_size_of(style: Style) -> f32 {
    match style {
        Style::H1 => 15.0,
        Style::H2 => 12.0,
        Style::H3 => 10.5,
        Style::Code => 8.25,
        _ => 9.0,
    }
}

/// The same room Preview leaves above a line, so Edit breaks and spaces as it will print.
fn space_above(style: Style) -> f64 {
    match style {
        Style::H1 => 20.0,
        Style::H2 => 12.0,
        Style::H3 => 8.0,
        Style::Quote => 12.0,
        _ => 0.0,
    }
}

const PAGE_TOP: f64 = 60.0;
const PAGE_BOTTOM: f64 = 72.0;
const PARAGRAPH_GAP: f64 = 8.0;
const LINE_WIDTH: f64 = 468.0;

fn floor_boundary(text: &str, mut at: usize) -> usize {
    at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The part of `new` an edit from `old` put in.
fn inserted_range(old: &str, new: &str) -> std::ops::Range<usize> {
    let prefix: usize = old.chars().zip(new.chars()).take_while(|(a, b)| a == b).map(|(c, _)| c.len_utf8()).sum();
    let suffix: usize = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum();
    prefix..new.len() - suffix
}

/// A Mac's pasteboard, for the context menu's Paste; ⌘V comes as text input.
#[cfg(target_os = "macos")]
fn pasteboard_text() -> Option<String> {
    use makepad_widgets::makepad_platform::makepad_objc_sys::{class, msg_send, runtime::Object, sel, sel_impl};
    use std::ffi::{c_char, CStr};
    unsafe {
        let pasteboard: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
        let kind: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c"public.utf8-plain-text".as_ptr()];
        let string: *mut Object = msg_send![pasteboard, stringForType: kind];
        if string.is_null() {
            return None;
        }
        let utf8: *const c_char = msg_send![string, UTF8String];
        if utf8.is_null() {
            return None;
        }
        Some(CStr::from_ptr(utf8).to_string_lossy().into_owned())
    }
}

#[cfg(not(target_os = "macos"))]
fn pasteboard_text() -> Option<String> {
    None
}

/// Where the press now under way began.
#[derive(Clone, Copy, Default, PartialEq)]
enum Press {
    /// Nowhere this page heard of.
    #[default]
    None,
    /// On the page.
    Page,
    /// Off the page with a row holding the keyboard.
    Away,
    /// On the page while another control or a menu held the keyboard.
    BehindOther,
    /// Off the page while something else had the keyboard (an open menu, or a field of the
    /// app's own such as a find bar's): what had it, and whether it was pressed.
    Beside(Area, bool),
}

#[derive(Script, ScriptHook, Widget)]
pub struct DocumentEditor {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    on_change: Option<ScriptFnRef>,
    #[live]
    on_format: Option<ScriptFnRef>,
    /// ⌘S on the page.
    #[live]
    on_save: Option<ScriptFnRef>,
    #[live]
    empty_text: String,
    /// Measures a quote, which sits in the middle of the line.
    #[live]
    measure: DrawText,
    /// The room above the page, which scrolls away with it.
    #[live]
    page_gap: f64,
    #[rust]
    document: Document,
    #[rust]
    meta: HashMap<String, Meta>,
    #[rust]
    selection: ArticleSelection,
    /// Rinx's selection wants a history; undo here is the draft's own.
    #[rust]
    spare_history: EditHistory,
    /// The draft as last told to the script.
    #[rust]
    text: String,
    #[rust]
    loaded: bool,
    #[rust]
    anchor: Position,
    #[rust]
    caret: Position,
    #[rust]
    pending: Pending,
    #[rust]
    pending_focus: bool,
    /// A right click moved the caret, and the menu it opened has the keyboard.
    #[rust]
    right_caret: bool,
    #[rust]
    focus_misses: u8,
    #[rust(true)]
    active: bool,
    #[rust]
    held: Vec<Held>,
    #[rust]
    undo: Vec<Snapshot>,
    #[rust]
    redo: Vec<Snapshot>,
    #[rust]
    last_kind: Option<EditKind>,
    #[rust]
    last_edit: Option<Instant>,
    #[rust]
    numbers: Vec<u64>,
    #[rust(true)]
    numbers_dirty: bool,
    #[rust]
    first_drawn: usize,
    #[rust]
    last_drawn: usize,
    #[rust]
    focus_area: Area,
    #[rust]
    reveal: u8,
    #[rust]
    frame: Option<NextFrame>,
    #[rust]
    notify_change: bool,
    #[rust]
    published: Option<(Style, bool, bool, usize, bool)>,
    #[rust]
    margin_press: Option<DVec2>,
    /// A press on the page that isn't on its text (the scroll bar): the page has the
    /// keyboard back when the press ends.
    #[rust]
    refocus_on_up: bool,
    #[rust]
    press: Press,
    #[rust]
    text_cursor: bool,
    #[rust]
    pending_replay_tries: u8,
}

impl DocumentEditor {
    fn list(&self, cx: &Cx) -> PortalListRef {
        self.view.portal_list(cx, ids!(blocks))
    }

    fn style(&self, index: usize) -> Style {
        self.document.blocks.get(index).and_then(|b| self.meta.get(&b.id)).map_or(Style::Body, |m| m.style)
    }

    fn marker(&self, index: usize) -> &str {
        self.document.blocks.get(index).and_then(|b| self.meta.get(&b.id)).map_or("", |m| m.marker.as_str())
    }

    /// Where the paper's top edge is now.
    fn paper_top(&self, cx: &Cx) -> f64 {
        let list = self.list(cx);
        if list.first_id() > 0 {
            return 0.0;
        }
        (self.page_gap + list.scroll_position().min(0.0)).max(0.0)
    }

    /// A row with a bold or slanted word is laid out word by word, which leaves a line's
    /// leading under its last line as well; a plain row has none there. This is how much,
    /// and the row's gap gives it back, so making a word bold moves nothing below it.
    fn marked_leading(&self, cx: &mut Cx, style: Style) -> f64 {
        let spacing = match style {
            Style::Code => return 0.0,
            Style::H1 | Style::H2 | Style::H3 => 1.25,
            _ => 1.3,
        };
        let laid = self.measure.layout(cx, 0.0, 0.0, None, false, Align::default(), "");
        let Some(row) = laid.rows.first() else { return 0.0 };
        let line = (row.ascender_in_lpxs - row.descender_in_lpxs + row.line_gap_in_lpxs) as f64;
        line * (font_size_of(style) as f64 / 9.0) * (spacing - 1.0)
    }

    fn row_input(&self, cx: &Cx, list: &PortalListRef, index: usize) -> Option<ArticleRichInputRef> {
        let (_, row) = list.get_item(index)?;
        let input = row.article_rich_input(cx, ids!(rich));
        (!input.is_empty()).then_some(input)
    }

    fn focused_row(&self, cx: &Cx, list: &PortalListRef) -> Option<(usize, ArticleRichInputRef)> {
        if self.last_drawn < self.first_drawn {
            return None;
        }
        (self.first_drawn..=self.last_drawn)
            .find_map(|index| self.row_input(cx, list, index).filter(|input| input.has_focus(cx)).map(|input| (index, input)))
    }

    /// A menu or another control, rather than the page, holds the keyboard.
    fn foreign_focus(&self, cx: &Cx, list: &PortalListRef) -> bool {
        let focus = cx.key_focus();
        !focus.is_empty() && focus != self.focus_area && self.focused_row(cx, list).is_none()
    }

    fn want_frame(&mut self, cx: &mut Cx) {
        if self.frame.is_none() {
            self.frame = Some(cx.new_next_frame());
        }
    }

    // ---- the draft ----------------------------------------------------------

    fn load_lines(&mut self, draft_text: &str) {
        self.document.blocks.clear();
        self.meta.clear();
        for line in draft::parse_draft(draft_text) {
            let mut block = Block::new(BlockKind::Paragraph, line.text);
            block.marks = line.marks;
            self.meta.insert(block.id.clone(), Meta { style: line.style, marker: line.marker, source: line.source });
            self.document.blocks.push(block);
        }
        self.text = draft_text.to_string();
        self.numbers_dirty = true;
        self.loaded = true;
    }

    /// A different document: its start, nothing to undo.
    fn load(&mut self, cx: &mut Cx, draft_text: &str) {
        self.load_lines(draft_text);
        self.selection.reset();
        self.undo.clear();
        self.redo.clear();
        self.last_kind = None;
        self.last_edit = None;
        self.anchor = Position::default();
        self.caret = Position::default();
        self.pending = Pending::default();
        self.published = None;
        self.held.clear();
        self.right_caret = false;
        self.list(cx).set_first_id_and_scroll(0, 0.0);
        self.want_frame(cx);
        self.view.redraw(cx);
    }

    /// Back to a draft of this document, keeping the rows that didn't change.
    fn reconcile(&mut self, draft_text: &str) {
        let lines = draft::parse_draft(draft_text);
        let count = lines.len();
        for (index, line) in lines.into_iter().enumerate() {
            if let Some(block) = self.document.blocks.get_mut(index) {
                let same = self.meta.get(&block.id).is_some_and(|m| m.source == line.source && m.style == line.style);
                if !same {
                    block.text = line.text;
                    block.marks = line.marks;
                    self.meta.insert(block.id.clone(), Meta { style: line.style, marker: line.marker, source: line.source });
                }
            } else {
                let mut block = Block::new(BlockKind::Paragraph, line.text);
                block.marks = line.marks;
                self.meta.insert(block.id.clone(), Meta { style: line.style, marker: line.marker, source: line.source });
                self.document.blocks.push(block);
            }
        }
        for block in self.document.blocks.drain(count..) {
            self.meta.remove(&block.id);
        }
        self.numbers_dirty = true;
    }

    fn join_sources(&self) -> String {
        let mut out = String::with_capacity(self.text.len() + 16);
        for (index, block) in self.document.blocks.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            if let Some(meta) = self.meta.get(&block.id) {
                out.push_str(&meta.source);
            }
        }
        out
    }

    /// The line again from what's on the page. Should Markdown not be able to say
    /// it, the page takes what the draft will read as: it never shows what isn't kept.
    fn refresh_source(&mut self, index: usize) {
        let Some(block) = self.document.blocks.get_mut(index) else { return };
        let Some(meta) = self.meta.get_mut(&block.id) else { return };
        meta.source = draft::emit_line(meta.style, &meta.marker, &block.text, &block.marks);
        if meta.style != Style::Code {
            let read = draft::parse_line(&meta.source);
            if read.text == block.text && !draft::same_ink(&block.text, &read.marks, &block.marks) {
                block.marks = read.marks;
            }
        }
    }

    /// Styles depend on the lines around: a fence's lines are code.
    fn reparse_all(&mut self) {
        let joined = self.join_sources();
        self.reconcile(&joined);
    }

    fn commit(&mut self, cx: &mut Cx, kind: EditKind) {
        let next = self.join_sources();
        if next != self.text {
            let now = Instant::now();
            let extends = kind != EditKind::Other
                && self.last_kind == Some(kind)
                && self.last_edit.is_some_and(|t| now.duration_since(t).as_secs_f64() < 1.0);
            let before = std::mem::replace(&mut self.text, next);
            if !extends {
                self.undo.push(Snapshot { text: before, anchor: self.anchor, caret: self.caret });
                if self.undo.len() > 100 {
                    self.undo.remove(0);
                }
            }
            self.redo.clear();
            self.last_kind = Some(kind);
            self.last_edit = Some(now);
            self.notify_change = true;
        }
        if kind == EditKind::Other {
            self.numbers_dirty = true;
        }
        self.reveal = 3;
        self.want_frame(cx);
        self.view.redraw(cx);
    }

    fn undo_redo(&mut self, cx: &mut Cx, redo: bool) {
        let popped = if redo { self.redo.pop() } else { self.undo.pop() };
        let Some(snapshot) = popped else { return };
        let current = Snapshot { text: self.text.clone(), anchor: self.anchor, caret: self.caret };
        if redo {
            self.undo.push(current);
        } else {
            self.redo.push(current);
        }
        self.reconcile(&snapshot.text);
        self.text = snapshot.text;
        self.notify_change = true;
        self.last_kind = None;
        self.pending = Pending::default();
        let (anchor, caret) = (self.clamp(snapshot.anchor), self.clamp(snapshot.caret));
        self.focus_at(cx, anchor, caret);
        // What comes back comes back selected, across its rows too.
        if anchor.block != caret.block {
            self.selection.selection = Some(BodySelection { anchor, cursor: caret });
        }
        self.reveal = 3;
    }

    fn clamp(&self, at: Position) -> Position {
        let block = at.block.min(self.document.blocks.len().saturating_sub(1));
        let text = self.document.blocks.get(block).map_or("", |b| b.text.as_str());
        Position { block, byte: floor_boundary(text, at.byte) }
    }

    fn sync_input(&self, cx: &mut Cx, list: &PortalListRef, index: usize) {
        let Some(block) = self.document.blocks.get(index) else { return };
        if let Some(input) = self.row_input(cx, list, index) {
            input.set_block(cx, block, font_size_of(self.style(index)));
        }
    }

    /// The caret goes here when the row is next on the page.
    fn focus_at(&mut self, cx: &mut Cx, anchor: Position, caret: Position) {
        self.selection.selection = None;
        self.anchor = anchor;
        self.caret = caret;
        self.pending_focus = true;
        self.focus_misses = 0;
        self.view.redraw(cx);
    }

    fn collapse_at(&mut self, cx: &mut Cx, at: Position) {
        self.focus_at(cx, at, at);
    }

    /// Where on the page a point is: the row nearest it, which may be a gap away.
    fn position_at(&self, cx: &Cx, list: &PortalListRef, abs: DVec2) -> Option<Position> {
        if self.last_drawn < self.first_drawn {
            return None;
        }
        (self.first_drawn..=self.last_drawn)
            .filter_map(|index| {
                let input = self.row_input(cx, list, index)?;
                let area = input.area();
                if !area.is_valid(cx) {
                    return None;
                }
                let rect = area.rect(cx);
                let distance = (rect.pos.y - abs.y).max(0.0) + (abs.y - rect.pos.y - rect.size.y).max(0.0);
                Some((distance, Position { block: index, byte: input.cursor_at(cx, abs)?.index }))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, at)| at)
    }

    /// A right click off the selection selects the word under it or puts the caret there,
    /// as a Mac's text views do, and the menu's commands act on that. On the selection it
    /// leaves it be. The keyboard goes to the menu first and comes back to this row after.
    fn right_press(&mut self, cx: &mut Cx, list: &PortalListRef, abs: DVec2) {
        if self.pending_focus || self.foreign_focus(cx, list) || !list.area().rect(cx).contains(abs) {
            return;
        }
        let Some(at) = self.position_at(cx, list, abs) else { return };
        let (start, end) = self.current_selection(cx, list).ordered();
        if start != end && start <= at && at <= end {
            return;
        }
        let Some(input) = self.row_input(cx, list, at.block) else { return };
        let word = self.document.blocks.get(at.block).and_then(|block| draft::word_around(&block.text, at.byte));
        let (from, to) = word.map_or((at.byte, at.byte), |word| (word.start, word.end));
        self.selection.selection = None;
        self.anchor = Position { block: at.block, byte: from };
        self.caret = Position { block: at.block, byte: to };
        self.pending = Pending::default();
        self.last_kind = None;
        input.set_selection(cx, Selection {
            anchor: Cursor { index: from, prefer_next_row: false },
            cursor: Cursor { index: to, prefer_next_row: false },
        });
        self.right_caret = true;
        self.view.redraw(cx);
    }

    // ---- numbered lists -----------------------------------------------------

    fn recount(&mut self) {
        self.numbers.clear();
        let mut counts = [None::<u64>; 2];
        for block in &self.document.blocks {
            let mut shown = 0;
            match self.meta.get(&block.id) {
                Some(meta) if meta.style == Style::Number => {
                    let level = draft::list_level(&meta.marker);
                    let literal = draft::number_of(&meta.marker).map_or(1, |n| n.0);
                    let n = counts[level].map_or(literal, |c| c + 1);
                    counts[level] = Some(n);
                    if level == 0 {
                        counts[1] = None;
                    }
                    shown = n;
                }
                Some(meta) if meta.style == Style::Bullet => {
                    let level = draft::list_level(&meta.marker);
                    counts[level] = None;
                    if level == 0 {
                        counts[1] = None;
                    }
                }
                _ => counts = [None, None],
            }
            self.numbers.push(shown);
        }
        self.numbers_dirty = false;
    }

    /// Writes the numbers of the list around `index` as they're shown.
    fn renumber(&mut self, index: usize) {
        let count = self.document.blocks.len();
        if index >= count {
            return;
        }
        let mut start = index;
        while start > 0 && self.style(start - 1).is_list() {
            start -= 1;
        }
        let mut counts = [None::<u64>; 2];
        let mut at = start;
        while at < count && self.style(at).is_list() {
            let marker = self.marker(at).to_string();
            let level = draft::list_level(&marker);
            if self.style(at) == Style::Number {
                if let Some((literal, rest)) = draft::number_of(&marker) {
                    let n = counts[level].map_or(literal, |c| c + 1);
                    counts[level] = Some(n);
                    if n != literal {
                        let indent = &marker[..marker.len() - marker.trim_start().len()];
                        let next = format!("{indent}{n}{rest}");
                        let id = self.document.blocks[at].id.clone();
                        if let Some(meta) = self.meta.get_mut(&id) {
                            meta.marker = next;
                        }
                        self.refresh_source(at);
                    }
                }
            } else {
                counts[level] = None;
            }
            if level == 0 {
                counts[1] = None;
            }
            at += 1;
        }
        self.numbers_dirty = true;
    }

    /// A new numbered line's marker: on from the line before it.
    fn number_marker(&self, index: usize, indent: &str) -> String {
        if index > 0 && self.style(index - 1) == Style::Number {
            let previous = self.marker(index - 1);
            if draft::list_level(previous) == draft::list_level(indent) {
                if let Some((n, rest)) = draft::number_of(previous) {
                    return format!("{indent}{}{rest}", n + 1);
                }
            }
        }
        format!("{indent}{}", Style::Number.marker())
    }

    // ---- what the selection is ----------------------------------------------

    fn current_selection(&self, cx: &Cx, list: &PortalListRef) -> BodySelection {
        if let Some(selection) = self.selection.selection {
            return selection;
        }
        if !self.pending_focus && !self.right_caret {
            if let Some((index, input)) = self.focused_row(cx, list) {
                let local = input.selection();
                return BodySelection {
                    anchor: Position { block: index, byte: local.anchor.index },
                    cursor: Position { block: index, byte: local.cursor.index },
                };
            }
        }
        BodySelection { anchor: self.anchor, cursor: self.caret }
    }

    fn checked_selection(&self, cx: &Cx, list: &PortalListRef) -> Option<BodySelection> {
        self.current_selection(cx, list).normalized(&self.document)
    }

    /// What the next typed character would be.
    fn typing_flags(&self, at: Position) -> (bool, bool) {
        let Some(block) = self.document.blocks.get(at.block) else { return (false, false) };
        let before = block.text[..floor_boundary(&block.text, at.byte)].char_indices().next_back().map_or(0, |(i, _)| i);
        let (mut bold, mut italic) = draft::flags_at(&block.marks, before);
        if let Some(value) = self.pending.bold {
            bold = value;
        }
        if let Some(value) = self.pending.italic {
            italic = value;
        }
        (bold, italic)
    }

    fn selected_text(&self, selection: BodySelection) -> String {
        let Some(selection) = selection.normalized(&self.document) else { return String::new() };
        self.document
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| selection.range(index, block.text.len()).map(|range| block.text[range].to_string()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Follows where the caret went, once a key or a press is done with.
    fn track(&mut self, cx: &mut Cx, list: &PortalListRef, by_key: bool) {
        if self.pending_focus || self.right_caret {
            return;
        }
        let (anchor, caret) = if let Some(mut selection) = self.selection.selection {
            if let Some((index, input)) = self.focused_row(cx, list) {
                let local = input.selection().cursor.index;
                if index == selection.cursor.block && local != selection.cursor.byte {
                    selection.cursor.byte = local;
                    self.selection.selection = Some(selection);
                    self.view.redraw(cx);
                }
            }
            (selection.anchor, selection.cursor)
        } else if let Some((index, input)) = self.focused_row(cx, list) {
            let local = input.selection();
            (Position { block: index, byte: local.anchor.index }, Position { block: index, byte: local.cursor.index })
        } else {
            return;
        };
        if anchor != self.anchor || caret != self.caret {
            if caret.block != self.caret.block {
                self.view.redraw(cx);
            }
            self.anchor = anchor;
            self.caret = caret;
            self.pending = Pending::default();
            self.last_kind = None;
            if by_key {
                self.reveal = 3;
                self.want_frame(cx);
            }
        }
    }

    fn publish(&mut self, cx: &mut Cx) {
        if self.notify_change {
            self.notify_change = false;
            if let (Some(handler), Some(vm_id)) = (self.on_change.clone(), cx.script_ref_vm_id(&self.source)) {
                let text = cx.with_script_vm_id(vm_id, |vm| ScriptValue::from(vm.bx.heap.new_string_from_str(&self.text)));
                cx.widget_to_script_call(self.widget_uid(), NIL, self.source.clone(), handler, &[text]);
            }
        }
        if self.pending_focus || !self.loaded {
            return;
        }
        let (start, end) = (self.anchor.min(self.caret), self.anchor.max(self.caret));
        let (bold, italic) = if start == end {
            self.typing_flags(start)
        } else {
            let flags = self.document.blocks.get(start.block).map_or((false, false), |b| draft::flags_at(&b.marks, start.byte));
            flags
        };
        // The last: whether there's a selection, for an Edit menu's Delete and Transformations.
        let state = (self.style(start.block), bold, italic, self.caret.block, start != end);
        if self.published == Some(state) {
            return;
        }
        self.published = Some(state);
        if let (Some(handler), Some(vm_id)) = (self.on_format.clone(), cx.script_ref_vm_id(&self.source)) {
            let style = cx.with_script_vm_id(vm_id, |vm| ScriptValue::from(vm.bx.heap.new_string_from_str(state.0.id())));
            cx.widget_to_script_call(
                self.widget_uid(),
                NIL,
                self.source.clone(),
                handler,
                &[style, ScriptValue::from(bold), ScriptValue::from(italic), ScriptValue::from(state.3 as f64), ScriptValue::from(state.4)],
            );
        }
    }

    // ---- formatting ---------------------------------------------------------

    fn toggle_emphasis(&mut self, cx: &mut Cx, list: &PortalListRef, bold: bool) {
        let Some(selection) = self.checked_selection(cx, list) else { return };
        let (start, end) = selection.ordered();
        self.anchor = selection.anchor;
        self.caret = selection.cursor;
        // A button may have been what asked: the caret stays on the page either way.
        self.pending_focus = true;
        self.focus_misses = 0;
        self.published = None;
        self.view.redraw(cx);
        if start == end {
            let (is_bold, is_italic) = self.typing_flags(start);
            if bold {
                self.pending.bold = Some(!is_bold);
            } else {
                self.pending.italic = Some(!is_italic);
            }
            self.want_frame(cx);
            return;
        }
        let flags = draft::flags_at(&self.document.blocks[start.block].marks, start.byte);
        let want = !(if bold { flags.0 } else { flags.1 });
        for index in start.block..=end.block {
            if self.style(index) == Style::Code {
                continue;
            }
            let block = &mut self.document.blocks[index];
            let Some(range) = selection.range(index, block.text.len()).filter(|r| !r.is_empty()) else { continue };
            let _ = block.format(range, bold.then_some(want), (!bold).then_some(want), None);
            self.refresh_source(index);
            self.sync_input(cx, list, index);
        }
        self.commit(cx, EditKind::Other);
    }

    fn apply_style(&mut self, index: usize, style: Style) {
        let current = self.style(index);
        if current == Style::Code || style == Style::Code || current == style {
            return;
        }
        let marker = self.marker(index).to_string();
        let indent = if current.is_list() && style.is_list() { marker[..marker.len() - marker.trim_start().len()].to_string() } else { String::new() };
        let next = match style {
            Style::Number => self.number_marker(index, &indent),
            Style::Bullet => format!("{indent}{}", Style::Bullet.marker()),
            other => other.marker().to_string(),
        };
        let id = self.document.blocks[index].id.clone();
        if let Some(meta) = self.meta.get_mut(&id) {
            meta.style = style;
            meta.marker = next;
        }
        self.refresh_source(index);
    }

    fn set_style(&mut self, cx: &mut Cx, list: &PortalListRef, style: Style) {
        let Some(selection) = self.checked_selection(cx, list) else { return };
        let (start, end) = selection.ordered();
        for index in start.block..=end.block {
            self.apply_style(index, style);
        }
        self.renumber(start.block);
        self.renumber(end.block);
        if end.block + 1 < self.document.blocks.len() {
            self.renumber(end.block + 1);
        }
        self.commit(cx, EditKind::Other);
        let kept = self.selection.selection;
        self.anchor = selection.anchor;
        self.caret = selection.cursor;
        self.pending_focus = true;
        self.focus_misses = 0;
        self.selection.selection = kept;
        self.published = None;
    }

    // ---- structure ----------------------------------------------------------

    fn insert_after(&mut self, index: usize, text: String, marks: Vec<Mark>, style: Style, marker: String) {
        let mut block = Block::new(BlockKind::Paragraph, text);
        block.marks = marks;
        self.meta.insert(block.id.clone(), Meta { style, marker, source: String::new() });
        self.document.blocks.insert(index + 1, block);
        self.refresh_source(index + 1);
    }

    fn remove_row(&mut self, index: usize) -> Block {
        let block = self.document.blocks.remove(index);
        self.meta.remove(&block.id);
        block
    }

    fn to_body(&mut self, index: usize) {
        let id = self.document.blocks[index].id.clone();
        if let Some(meta) = self.meta.get_mut(&id) {
            meta.style = Style::Body;
            meta.marker = String::new();
        }
        self.refresh_source(index);
    }

    /// One step out of a list or a style: a nested item comes out a level, anything
    /// else becomes body text.
    fn outdent(&mut self, index: usize) {
        let style = self.style(index);
        let marker = self.marker(index).to_string();
        if style.is_list() && draft::list_level(&marker) > 0 {
            let id = self.document.blocks[index].id.clone();
            if let Some(meta) = self.meta.get_mut(&id) {
                meta.marker = marker.trim_start().to_string();
            }
            self.refresh_source(index);
        } else {
            self.to_body(index);
        }
        self.renumber(index);
        if index + 1 < self.document.blocks.len() {
            self.renumber(index + 1);
        }
    }

    fn split(&mut self, cx: &mut Cx, index: usize, start: usize, end: usize) {
        let style = self.style(index);
        let marker = self.marker(index).to_string();
        let block = &self.document.blocks[index];
        let (start, end) = (floor_boundary(&block.text, start), floor_boundary(&block.text, end));
        let head = block.text[..start].to_string();
        let tail = block.text[end..].to_string();
        let head_marks = draft::slice_marks(&block.marks, 0..start);
        let tail_marks = draft::slice_marks(&block.marks, end..block.text.len());
        let empty = head.is_empty() && tail.is_empty();
        if empty && (style.is_list() || style == Style::Quote) {
            self.outdent(index);
            self.commit(cx, EditKind::Other);
            self.collapse_at(cx, Position { block: index, byte: 0 });
            return;
        }
        if style.is_heading() && head.is_empty() && !tail.is_empty() {
            // A line opens above the heading, and the caret stays on the heading.
            let id = self.document.blocks[index].id.clone();
            let moved = self.document.blocks[index].clone();
            let moved_meta = self.meta.remove(&id);
            self.document.blocks[index] = Block::new(BlockKind::Paragraph, "");
            let fresh = self.document.blocks[index].id.clone();
            self.meta.insert(fresh, Meta { style: Style::Body, marker: String::new(), source: String::new() });
            self.document.blocks.insert(index + 1, moved);
            if let Some(meta) = moved_meta {
                self.meta.insert(id, meta);
            }
            self.commit(cx, EditKind::Other);
            self.collapse_at(cx, Position { block: index + 1, byte: 0 });
            return;
        }
        let (next_style, next_marker) = if style.is_heading() && tail.is_empty() {
            (Style::Body, String::new())
        } else if style == Style::Number {
            let indent = marker[..marker.len() - marker.trim_start().len()].to_string();
            let next = draft::number_of(&marker)
                .map_or_else(|| format!("{indent}{}", Style::Number.marker()), |(n, rest)| format!("{indent}{}{rest}", n + 1));
            (style, next)
        } else {
            (style, marker.clone())
        };
        {
            let block = &mut self.document.blocks[index];
            block.text = head;
            block.marks = head_marks;
        }
        self.refresh_source(index);
        self.insert_after(index, tail, tail_marks, next_style, next_marker);
        self.renumber(index);
        if style == Style::Code {
            self.reparse_all();
        }
        self.commit(cx, EditKind::Other);
        self.collapse_at(cx, Position { block: index + 1, byte: 0 });
    }

    fn merge_into_previous(&mut self, cx: &mut Cx, index: usize) {
        let was_code = self.style(index) == Style::Code || self.style(index - 1) == Style::Code;
        let removed = self.remove_row(index);
        let at = {
            let previous = &mut self.document.blocks[index - 1];
            let at = previous.text.len();
            previous.marks = draft::join_marks(&previous.marks, &removed.marks, at);
            previous.text.push_str(&removed.text);
            at
        };
        self.refresh_source(index - 1);
        self.renumber(index - 1);
        if index < self.document.blocks.len() {
            self.renumber(index);
        }
        if was_code {
            self.reparse_all();
        }
        self.commit(cx, EditKind::Other);
        let at = self.clamp(Position { block: index - 1, byte: at });
        self.collapse_at(cx, at);
    }

    fn backspace_at_start(&mut self, cx: &mut Cx, index: usize) {
        let style = self.style(index);
        if style != Style::Body && style != Style::Code {
            self.outdent(index);
            self.commit(cx, EditKind::Other);
            self.collapse_at(cx, Position { block: index, byte: 0 });
        } else if index > 0 {
            self.merge_into_previous(cx, index);
        }
    }

    fn delete_at_end(&mut self, cx: &mut Cx, index: usize) {
        if index + 1 >= self.document.blocks.len() {
            return;
        }
        if self.document.blocks[index].text.is_empty() && self.style(index) == Style::Body {
            self.remove_row(index);
            self.renumber(index);
            self.commit(cx, EditKind::Other);
            self.collapse_at(cx, Position { block: index, byte: 0 });
        } else {
            self.merge_into_previous(cx, index + 1);
        }
    }

    fn set_indent(&mut self, cx: &mut Cx, index: usize, deeper: bool) {
        let marker = self.marker(index).to_string();
        let bare = marker.trim_start();
        let next = if deeper { format!("   {bare}") } else { bare.to_string() };
        if next == marker {
            return;
        }
        let id = self.document.blocks[index].id.clone();
        if let Some(meta) = self.meta.get_mut(&id) {
            meta.marker = next;
        }
        self.refresh_source(index);
        self.renumber(index);
        self.commit(cx, EditKind::Other);
    }

    /// Replaces a selection, in one row or across rows, and says where the caret is.
    fn replace_selection(&mut self, cx: &mut Cx, selection: BodySelection, text: &str, whole_row: bool) -> Option<Position> {
        let selection = selection.normalized(&self.document)?;
        let position = selection.replace(&mut self.document, text)?;
        self.selection.selection = None;
        let live: HashSet<&String> = self.document.blocks.iter().map(|b| &b.id).collect();
        self.meta.retain(|id, _| live.contains(id));
        let position = self.split_lines(position, whole_row);
        self.refresh_source(position.block);
        self.renumber(position.block);
        if position.block + 1 < self.document.blocks.len() {
            self.renumber(position.block + 1);
        }
        let _ = cx;
        Some(self.clamp(position))
    }

    /// Pasted lines become rows. Whole lines read as draft lines do, so pasted
    /// Markdown comes out formatted.
    fn split_lines(&mut self, at: Position, parse_first: bool) -> Position {
        let index = at.block;
        let block = &self.document.blocks[index];
        if !block.text.contains('\n') {
            if parse_first && self.style(index) == Style::Body {
                let line = draft::parse_line(&block.text.clone());
                let tail = block.text.len() - at.byte;
                self.adopt_line(index, line);
                let block = &self.document.blocks[index];
                return Position { block: index, byte: floor_boundary(&block.text, block.text.len().saturating_sub(tail)) };
            }
            return at;
        }
        let text = block.text.clone();
        let marks = block.marks.clone();
        let tail = text.len() - at.byte;
        let parts: Vec<&str> = text.split('\n').map(|p| p.strip_suffix('\r').unwrap_or(p)).collect();
        if parse_first && self.style(index) == Style::Body {
            self.adopt_line(index, draft::parse_line(parts[0]));
        } else {
            let block = &mut self.document.blocks[index];
            block.text = parts[0].to_string();
            block.marks = draft::slice_marks(&marks, 0..parts[0].len());
            self.refresh_source(index);
        }
        for (k, part) in parts[1..].iter().enumerate() {
            let line = draft::parse_line(part);
            self.insert_after(index + k, line.text, line.marks, line.style, line.marker);
        }
        let last = index + parts.len() - 1;
        self.reparse_all();
        let block = &self.document.blocks[last];
        Position { block: last, byte: floor_boundary(&block.text, block.text.len().saturating_sub(tail)) }
    }

    fn adopt_line(&mut self, index: usize, line: draft::Line) {
        let block = &mut self.document.blocks[index];
        block.text = line.text;
        block.marks = line.marks;
        let id = block.id.clone();
        self.meta.insert(id, Meta { style: line.style, marker: line.marker, source: line.source });
    }

    /// The menu's and the keyboard's way to put text in, wherever the caret is.
    fn insert_text(&mut self, cx: &mut Cx, list: &PortalListRef, text: &str) {
        let Some(selection) = self.checked_selection(cx, list) else { return };
        let (start, end) = selection.ordered();
        let empty_row = start == end && self.document.blocks[start.block].text.is_empty();
        if let Some(position) = self.replace_selection(cx, selection, text, empty_row) {
            self.commit(cx, EditKind::Other);
            self.collapse_at(cx, position);
            self.reveal = 3;
            self.want_frame(cx);
        }
    }

    fn cut(&mut self, cx: &mut Cx, list: &PortalListRef) {
        let Some(selection) = self.checked_selection(cx, list) else { return };
        let (start, end) = selection.ordered();
        if start == end {
            return;
        }
        cx.copy_to_clipboard(&self.selected_text(selection));
        self.insert_text(cx, list, "");
    }

    fn copy(&mut self, cx: &mut Cx, list: &PortalListRef) {
        let selection = self.current_selection(cx, list);
        let text = self.selected_text(selection);
        if !text.is_empty() {
            cx.copy_to_clipboard(&text);
        }
    }

    /// Edit's Delete: the selection goes, and the pasteboard keeps what it had.
    fn delete_selected(&mut self, cx: &mut Cx, list: &PortalListRef) {
        let Some(selection) = self.checked_selection(cx, list) else { return };
        let (start, end) = selection.ordered();
        if start != end {
            self.insert_text(cx, list, "");
        }
    }

    /// `words` in place of the selection as they are, in the bold and italic given;
    /// says where they end.
    fn put_plain(&mut self, selection: BodySelection, words: &str, flags: (bool, bool)) -> Option<Position> {
        let selection = selection.normalized(&self.document)?;
        let (start, _) = selection.ordered();
        let at = selection.replace(&mut self.document, words)?;
        self.selection.selection = None;
        let live: HashSet<&String> = self.document.blocks.iter().map(|b| &b.id).collect();
        self.meta.retain(|id, _| live.contains(id));
        if (flags.0 || flags.1) && at.byte > start.byte && self.style(start.block) != Style::Code {
            let _ = self.document.blocks[start.block].format(start.byte..at.byte, Some(flags.0), Some(flags.1), None);
        }
        Some(at)
    }

    /// Paste and Match Style: the pasteboard's words as if typed where they go. Markdown in
    /// them stays characters, they take the bold or italic of the place, and each line after
    /// the first is a row as Return would make it: another item of a list, body text after a
    /// heading.
    fn paste_matching(&mut self, cx: &mut Cx, list: &PortalListRef, text: &str) {
        let Some(selection) = self.checked_selection(cx, list) else { return };
        let (start, end) = selection.ordered();
        let flags = if start == end {
            self.typing_flags(start)
        } else {
            draft::flags_at(&self.document.blocks[start.block].marks, start.byte)
        };
        let lines: Vec<&str> = text.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)).collect();
        let Some(mut at) = self.put_plain(selection, lines[0], flags) else { return };
        let index = at.block;
        let style = self.style(index);
        if lines.len() > 1 {
            let marker = self.marker(index).to_string();
            let (next_style, next_marker) = if style.is_heading() { (Style::Body, String::new()) } else { (style, marker) };
            // The rest of the row goes after the last line.
            let (tail, tail_marks) = {
                let block = &mut self.document.blocks[index];
                let tail = block.text.split_off(at.byte);
                let tail_marks = draft::slice_marks(&block.marks, at.byte..at.byte + tail.len());
                block.marks = draft::slice_marks(&block.marks, 0..at.byte);
                (tail, tail_marks)
            };
            self.refresh_source(index);
            let last = lines.len() - 1;
            for (k, line) in lines.iter().enumerate().skip(1) {
                let mut marks = Vec::new();
                if (flags.0 || flags.1) && !line.is_empty() && next_style != Style::Code {
                    marks.push(Mark { start: 0, end: line.len(), bold: flags.0, italic: flags.1, link: None });
                }
                let mut words = line.to_string();
                if k == last {
                    marks = draft::join_marks(&marks, &tail_marks, words.len());
                    words.push_str(&tail);
                }
                self.insert_after(index + k - 1, words, marks, next_style, next_marker.clone());
            }
            at = Position { block: index + last, byte: lines[last].len() };
            self.renumber(index);
            if style == Style::Code {
                self.reparse_all();
            }
        } else {
            self.refresh_source(index);
        }
        self.commit(cx, EditKind::Other);
        let at = self.clamp(at);
        self.collapse_at(cx, at);
        self.reveal = 3;
        self.want_frame(cx);
    }

    /// Find and Replace: `with` in place of the selected match of `words`, then on to the
    /// next (a selection that isn't a match is only moved on from); with `all`, in place of
    /// every match, as one change to undo. The replacing words take the bold and italic of
    /// what they replace. Says the match chosen after, as `find` does, and how many were
    /// replaced; the keyboard stays where it is.
    fn replace_found(&mut self, cx: &mut Cx, list: &PortalListRef, words: &str, with: &str, all: bool) -> (usize, usize, usize) {
        let found = draft::find_all(self.document.blocks.iter().map(|b| b.text.as_str()), words);
        let put = |this: &mut Self, (block, from, to): draft::Found| {
            let flags = draft::flags_at(&this.document.blocks[block].marks, from);
            let selection = BodySelection { anchor: Position { block, byte: from }, cursor: Position { block, byte: to } };
            let at = this.put_plain(selection, with, flags);
            this.refresh_source(block);
            at
        };
        if all {
            if found.is_empty() {
                return (0, 0, 0);
            }
            // From the last, so the places before it stay where they are.
            let mut first = None;
            for f in found.iter().rev() {
                first = put(self, *f);
            }
            self.commit(cx, EditKind::Other);
            if let Some(at) = first {
                let at = self.clamp(at);
                self.anchor = at;
                self.caret = at;
            }
            self.selection.selection = None;
            self.pending_focus = false;
            self.view.redraw(cx);
            let left = draft::find_all(self.document.blocks.iter().map(|b| b.text.as_str()), words).len();
            return (0, left, found.len());
        }
        let (start, end) = self.current_selection(cx, list).ordered();
        let mut replaced = 0;
        if let Some(f) = found.iter().find(|f| start == Position { block: f.0, byte: f.1 } && end == Position { block: f.0, byte: f.2 }) {
            if let Some(at) = put(self, *f) {
                self.commit(cx, EditKind::Other);
                let at = self.clamp(at);
                self.anchor = at;
                self.caret = at;
                replaced = 1;
            }
        }
        let (index, count) = self.find(cx, list, words, 0);
        (index, count, replaced)
    }

    /// Transformations: the selection's letters in capitals, in small letters, or a capital
    /// to each word. The selection stays on them, and their bold and italic with them.
    fn change_case(&mut self, cx: &mut Cx, list: &PortalListRef, case: draft::Case) {
        let Some(selection) = self.checked_selection(cx, list) else { return };
        let (start, end) = selection.ordered();
        if start == end {
            return;
        }
        let (mut anchor, mut caret) = (selection.anchor, selection.cursor);
        let mut changed = Vec::new();
        for index in start.block..=end.block {
            let block = &self.document.blocks[index];
            let Some(range) = selection.range(index, block.text.len()).filter(|r| !r.is_empty()) else { continue };
            // A word starts a line, and after a space.
            let word_start = block.text[..range.start].chars().next_back().map_or(true, char::is_whitespace);
            let (text, moved) = draft::change_case(&block.text, range, case, word_start);
            if text == block.text {
                continue;
            }
            let block = &mut self.document.blocks[index];
            block.text = text;
            for mark in &mut block.marks {
                mark.start = draft::moved_to(&moved, mark.start);
                mark.end = draft::moved_to(&moved, mark.end);
            }
            for at in [&mut anchor, &mut caret] {
                if at.block == index {
                    at.byte = draft::moved_to(&moved, at.byte);
                }
            }
            changed.push(index);
        }
        if changed.is_empty() {
            return;
        }
        for index in changed {
            self.refresh_source(index);
        }
        self.commit(cx, EditKind::Other);
        self.focus_at(cx, anchor, caret);
        if anchor.block != caret.block {
            self.selection.selection = Some(BodySelection { anchor, cursor: caret });
        }
    }

    // ---- what a row just did ------------------------------------------------

    fn input_changed(&mut self, cx: &mut Cx, list: &PortalListRef, index: usize, input: &ArticleRichInputRef) {
        let Some((text, marks)) = input.content() else { return };
        let Some(block) = self.document.blocks.get_mut(index) else { return };
        if block.text == text && block.marks == marks {
            return;
        }
        let old = std::mem::take(&mut block.text);
        let caret = input.selection().cursor.index;
        let kind = if text.len() < old.len() { EditKind::Deleting } else { EditKind::Typing };
        let inserted = inserted_range(&old, &text);
        block.text = text;
        block.marks = marks;
        let mut restyled = false;
        if (self.pending.bold.is_some() || self.pending.italic.is_some()) && !inserted.is_empty() {
            let _ = block.format(inserted, self.pending.bold, self.pending.italic, None);
            self.pending = Pending::default();
            restyled = true;
        }
        if self.document.blocks[index].text.contains('\n') {
            let position = self.split_lines(Position { block: index, byte: caret }, old.is_empty());
            self.renumber(position.block);
            self.commit(cx, EditKind::Other);
            self.collapse_at(cx, position);
            return;
        }
        let style = self.style(index);
        let now_text = self.document.blocks[index].text.clone();
        if style == Style::Body {
            if draft::is_table_row(&now_text) || draft::fence_of(&now_text).is_some() {
                let id = self.document.blocks[index].id.clone();
                self.document.blocks[index].marks.clear();
                if let Some(meta) = self.meta.get_mut(&id) {
                    meta.style = Style::Code;
                    meta.marker = String::new();
                }
            } else if let Some((next, len)) = draft::typed_marker(&now_text, caret) {
                // Typed `- `: the marker goes, and the line is a list item.
                let id = self.document.blocks[index].id.clone();
                let block = &mut self.document.blocks[index];
                block.marks = draft::slice_marks(&block.marks, len..now_text.len());
                block.text = now_text[len..].to_string();
                if let Some(meta) = self.meta.get_mut(&id) {
                    meta.style = next;
                    meta.marker = now_text[..len].to_string();
                }
                self.refresh_source(index);
                self.renumber(index);
                self.published = None;
                self.commit(cx, EditKind::Other);
                self.collapse_at(cx, Position { block: index, byte: 0 });
                return;
            }
        }
        if self.style(index) == Style::Code {
            let id = self.document.blocks[index].id.clone();
            if let Some(meta) = self.meta.get_mut(&id) {
                meta.source = now_text;
            }
            self.reparse_all();
            let at = self.clamp(Position { block: index, byte: caret });
            self.commit(cx, kind);
            if self.style(index) != style {
                self.published = None;
                self.collapse_at(cx, at);
            } else {
                self.anchor = at;
                self.caret = at;
            }
            return;
        }
        let mut at = caret;
        let block = &mut self.document.blocks[index];
        if let Some(live) = draft::live_emphasis(&block.text, &block.marks, caret) {
            block.text = live.text;
            block.marks = live.marks;
            at = live.caret;
            restyled = true;
            self.pending = Pending { bold: Some(false), italic: Some(false) };
        }
        self.refresh_source(index);
        if restyled || self.document.blocks[index].marks != input.content().map_or(Vec::new(), |c| c.1) {
            self.sync_input(cx, list, index);
            input.set_selection(cx, Selection {
                anchor: Cursor { index: at, prefer_next_row: false },
                cursor: Cursor { index: at, prefer_next_row: false },
            });
        }
        // The caret before this edit goes with the undo step; then it moves.
        self.commit(cx, kind);
        self.anchor = Position { block: index, byte: at };
        self.caret = self.anchor;
    }

    /// An arrow the row had no more room for goes on to the next row.
    fn arrow_out(&mut self, cx: &mut Cx, list: &PortalListRef, index: usize, input: &ArticleRichInputRef, key: &KeyEvent) {
        if key.modifiers.logo || key.modifiers.control {
            return;
        }
        let count = self.document.blocks.len();
        let local = input.selection();
        let x_and_rect = input.cursor_rect_in_absolute(cx);
        let target = match key.key_code {
            KeyCode::ArrowUp if index == 0 => Position::default(),
            KeyCode::ArrowDown if index + 1 == count => Position { block: index, byte: self.document.blocks[index].text.len() },
            KeyCode::ArrowUp | KeyCode::ArrowDown => {
                let up = key.key_code == KeyCode::ArrowUp;
                let next = if up { index - 1 } else { index + 1 };
                let len = self.document.blocks[next].text.len();
                let byte = x_and_rect
                    .and_then(|rect| {
                        let neighbour = self.row_input(cx, list, next)?;
                        let area = neighbour.area();
                        if !area.is_valid(cx) {
                            return None;
                        }
                        let box_rect = area.rect(cx);
                        let y = if up { box_rect.pos.y + box_rect.size.y - 2.0 } else { box_rect.pos.y + 2.0 };
                        neighbour.cursor_at(cx, dvec2(rect.pos.x, y)).map(|c| c.index)
                    })
                    .unwrap_or(if up { len } else { 0 });
                Position { block: next, byte }
            }
            KeyCode::ArrowLeft if index > 0 => Position { block: index - 1, byte: self.document.blocks[index - 1].text.len() },
            KeyCode::ArrowRight if index + 1 < count => Position { block: index + 1, byte: 0 },
            _ => return,
        };
        self.reveal = 3;
        self.want_frame(cx);
        if key.modifiers.shift {
            let anchor = self.selection.selection.map_or(
                Position { block: index, byte: local.anchor.index },
                |selection| selection.anchor,
            );
            self.selection.selection = Some(BodySelection { anchor, cursor: target });
            self.anchor = anchor;
            self.caret = target;
            self.pending_focus = true;
            self.focus_misses = 0;
            self.view.redraw(cx);
        } else {
            self.collapse_at(cx, target);
        }
    }

    fn scroll_by(&mut self, cx: &mut Cx, list: &PortalListRef, delta: f64) {
        if let Some(mut inner) = list.borrow_mut() {
            let first = inner.first_id();
            let scroll = inner.first_scroll();
            let next = if first == 0 { (scroll + delta).min(0.0) } else { scroll + delta };
            inner.set_first_id_and_scroll(first, next);
        }
        list.redraw(cx);
    }

    /// True when the key was this widget's to deal with.
    fn handle_key(&mut self, cx: &mut Cx, list: &PortalListRef, key: &KeyEvent) -> bool {
        let Some((index, input)) = self.focused_row(cx, list) else { return false };
        if input.is_composing() {
            return false;
        }
        let m = key.modifiers;
        let primary = m.is_primary();
        match key.key_code {
            KeyCode::KeyZ if primary => {
                self.undo_redo(cx, m.shift);
                return true;
            }
            KeyCode::KeyB if primary && !m.shift => {
                self.toggle_emphasis(cx, list, true);
                return true;
            }
            KeyCode::KeyI if primary && !m.shift => {
                self.toggle_emphasis(cx, list, false);
                return true;
            }
            KeyCode::KeyV if primary && m.shift && m.alt => {
                // Paste and Match Style, Option-Shift-Command-V as a Mac's text views have it
                // (the shell leaves Command-V's keys to a field that holds the keyboard).
                if let Some(text) = pasteboard_text() {
                    self.paste_matching(cx, list, &text);
                    self.want_frame(cx);
                }
                return true;
            }
            KeyCode::KeyS if primary && !m.shift => {
                if let Some(handler) = self.on_save.clone() {
                    cx.widget_to_script_call(self.widget_uid(), NIL, self.source.clone(), handler, &[]);
                }
                return true;
            }
            KeyCode::PageUp | KeyCode::PageDown if !m.any() => {
                let page = list.area().rect(cx).size.y - 40.0;
                self.scroll_by(cx, list, if key.key_code == KeyCode::PageUp { page } else { -page });
                return true;
            }
            KeyCode::ArrowUp | KeyCode::ArrowDown if primary => {
                let target = if key.key_code == KeyCode::ArrowUp {
                    Position::default()
                } else {
                    let last = self.document.blocks.len() - 1;
                    Position { block: last, byte: self.document.blocks[last].text.len() }
                };
                self.reveal = 3;
                self.want_frame(cx);
                if m.shift {
                    let anchor = self.current_selection(cx, list).anchor;
                    self.selection.selection = Some(BodySelection { anchor, cursor: target });
                    self.anchor = anchor;
                    self.caret = target;
                    self.pending_focus = true;
                    self.focus_misses = 0;
                    self.view.redraw(cx);
                } else {
                    self.collapse_at(cx, target);
                }
                return true;
            }
            _ => {}
        }
        if let Some(selection) = self.selection.selection {
            match key.key_code {
                KeyCode::ReturnKey | KeyCode::NumpadEnter => {
                    if let Some(at) = self.replace_selection(cx, selection, "", false) {
                        self.split(cx, at.block, at.byte, at.byte);
                    }
                    return true;
                }
                KeyCode::Backspace | KeyCode::Delete => {
                    if let Some(at) = self.replace_selection(cx, selection, "", false) {
                        self.commit(cx, EditKind::Other);
                        self.collapse_at(cx, at);
                    }
                    return true;
                }
                KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::ArrowUp | KeyCode::ArrowDown if !m.shift => {
                    let (start, end) = selection.ordered();
                    let to = if matches!(key.key_code, KeyCode::ArrowLeft | KeyCode::ArrowUp) { start } else { end };
                    self.collapse_at(cx, to);
                    self.reveal = 3;
                    self.want_frame(cx);
                    return true;
                }
                _ => {}
            }
        }
        let local = input.selection();
        let (start, end) = (local.start().index, local.end().index);
        match key.key_code {
            KeyCode::ReturnKey | KeyCode::NumpadEnter => {
                self.split(cx, index, start, end);
                true
            }
            KeyCode::Backspace if start == 0 && end == 0 => {
                self.backspace_at_start(cx, index);
                true
            }
            KeyCode::Delete if start == end && end == self.document.blocks[index].text.len() => {
                self.delete_at_end(cx, index);
                true
            }
            KeyCode::Tab => {
                // The window's Tab has already asked for the next field.
                cx.set_key_focus(cx.key_focus());
                if self.style(index).is_list() {
                    self.set_indent(cx, index, !m.shift);
                } else if !m.shift {
                    self.insert_text(cx, list, "    ");
                }
                true
            }
            _ => false,
        }
    }

    // ---- frames -------------------------------------------------------------

    /// Keeps the caret on the page after a key moved it.
    fn reveal_caret(&mut self, cx: &mut Cx, list: &PortalListRef) {
        let viewport = list.area().rect(cx);
        if viewport.size.y <= 0.0 {
            return;
        }
        let block = self.caret.block;
        if self.last_drawn < self.first_drawn || block < self.first_drawn || block > self.last_drawn {
            let near_end = block > self.last_drawn;
            let at = if near_end { (viewport.size.y - 72.0).max(0.0) } else { 16.0 };
            list.set_first_id_and_scroll(block, at);
            list.redraw(cx);
            return;
        }
        let Some(input) = self.row_input(cx, list, block) else { return };
        let Some(rect) = input.cursor_rect_in_absolute(cx) else { return };
        let margin = 16.0;
        let top = viewport.pos.y + margin;
        let bottom = viewport.pos.y + viewport.size.y - margin;
        let delta = if rect.pos.y < top {
            top - rect.pos.y
        } else if rect.pos.y + rect.size.y > bottom {
            bottom - rect.pos.y - rect.size.y
        } else {
            0.0
        };
        if delta != 0.0 {
            self.scroll_by(cx, list, delta);
        }
    }

    fn on_frame(&mut self, cx: &mut Cx, list: &PortalListRef, scope: &mut Scope) {
        self.frame = None;
        if self.pending_focus {
            self.view.redraw(cx);
        }
        if !self.pending_focus && !self.held.is_empty() {
            // Focus moves between events: the row may not have it yet.
            if self.focused_row(cx, list).is_some() || self.pending_replay_tries >= 8 {
                self.pending_replay_tries = 0;
                for held in std::mem::take(&mut self.held) {
                    match held {
                        Held::Key(key) => self.handle_event(cx, &Event::KeyDown(key), scope),
                        Held::Text(text) => self.handle_event(cx, &Event::TextInput(text), scope),
                    }
                }
            } else {
                self.pending_replay_tries += 1;
                self.want_frame(cx);
            }
        }
        if self.reveal > 0 && !self.pending_focus {
            self.reveal -= 1;
            self.reveal_caret(cx, list);
            if self.reveal > 0 {
                self.want_frame(cx);
            }
        }
    }

    fn set_active(&mut self, cx: &mut Cx, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        self.refocus_on_up = false;
        self.right_caret = false;
        self.press = Press::None;
        if active {
            self.pending_focus = true;
            self.focus_misses = 0;
            self.view.redraw(cx);
        } else {
            self.pending_focus = false;
            let list = self.list(cx);
            self.track(cx, &list, false);
            if !self.focus_area.is_empty() && cx.has_key_focus(self.focus_area) {
                cx.set_key_focus(Area::Empty);
            }
        }
    }

    fn go_to_line(&mut self, cx: &mut Cx, list: &PortalListRef, line: usize) {
        let at = self.clamp(Position { block: line, byte: 0 });
        self.collapse_at(cx, at);
        let viewport = list.area().rect(cx);
        let visible = at.block >= self.first_drawn && at.block <= self.last_drawn && self.row_input(cx, list, at.block).is_some_and(|input| {
            let rect = input.area().rect(cx);
            rect.pos.y >= viewport.pos.y && rect.pos.y + rect.size.y <= viewport.pos.y + viewport.size.y
        });
        if !visible {
            list.set_first_id_and_scroll(at.block, if at.block == 0 { 0.0 } else { 16.0 });
            list.redraw(cx);
        }
    }

    /// Find's match chosen, selected across the page as a selection is, and brought
    /// into view, while the keyboard stays where it is. (index from 1, count); none
    /// found drops the highlight and says (0, 0).
    fn find(&mut self, cx: &mut Cx, list: &PortalListRef, words: &str, step: i32) -> (usize, usize) {
        let found = draft::find_all(self.document.blocks.iter().map(|b| b.text.as_str()), words);
        let (start, end) = (self.anchor.min(self.caret), self.anchor.max(self.caret));
        let Some(i) = draft::pick_found(&found, (start.block, start.byte), (end.block, end.byte), step) else {
            if self.selection.selection.take().is_some() {
                self.view.redraw(cx);
            }
            return (0, 0);
        };
        let (block, from, to) = found[i];
        let (anchor, caret) = (Position { block, byte: from }, Position { block, byte: to });
        self.anchor = anchor;
        self.caret = caret;
        self.selection.selection = Some(BodySelection { anchor, cursor: caret });
        self.pending_focus = false;
        let viewport = list.area().rect(cx);
        let shown = block >= self.first_drawn && block <= self.last_drawn && self.row_input(cx, list, block).is_some_and(|input| {
            let rect = input.area().rect(cx);
            rect.pos.y >= viewport.pos.y && rect.pos.y + rect.size.y <= viewport.pos.y + viewport.size.y
        });
        if !shown {
            list.set_first_id_and_scroll(block, (viewport.size.y / 3.0).max(16.0));
            list.redraw(cx);
        }
        self.view.redraw(cx);
        (i + 1, found.len())
    }

    fn headings_json(&self) -> String {
        let mut out = String::from("[");
        for (index, block) in self.document.blocks.iter().enumerate() {
            let level = match self.style(index) {
                Style::H1 => 1,
                Style::H2 => 2,
                Style::H3 => 3,
                _ => continue,
            };
            let text = block.text.trim();
            if text.is_empty() {
                continue;
            }
            if out.len() > 1 {
                out.push(',');
            }
            out.push_str(&format!("{{\"level\":{level},\"text\":{},\"line\":{index}}}", draft::json_string(text)));
        }
        out.push(']');
        out
    }
}

fn first_string(vm: &mut ScriptVm, args: ScriptValue) -> Option<String> {
    args.as_object().and_then(|obj| {
        let trap = vm.bx.threads.cur().trap.pass();
        let value = vm.bx.heap.vec_value(obj, 0, trap);
        vm.bx.heap.cast_to_owned_string(value, "reading a document editor argument")
    })
}

fn nth_string(vm: &mut ScriptVm, args: ScriptValue, n: usize) -> Option<String> {
    args.as_object().and_then(|obj| {
        let trap = vm.bx.threads.cur().trap.pass();
        let value = vm.bx.heap.vec_value(obj, n, trap);
        vm.bx.heap.cast_to_owned_string(value, "reading a document editor argument")
    })
}

fn first_value(vm: &mut ScriptVm, args: ScriptValue) -> ScriptValue {
    nth_value(vm, args, 0)
}

fn nth_value(vm: &mut ScriptVm, args: ScriptValue, n: usize) -> ScriptValue {
    args.as_object().map_or(NIL, |obj| {
        let trap = vm.bx.threads.cur().trap.pass();
        vm.bx.heap.vec_value(obj, n, trap)
    })
}

impl Widget for DocumentEditor {
    fn script_call(&mut self, vm: &mut ScriptVm, method: LiveId, args: ScriptValue) -> ScriptAsyncResult {
        if method == live_id!(text) {
            return ScriptAsyncResult::Return(vm.bx.heap.new_string_from_str(&self.text).into());
        }
        if method == live_id!(set_text) {
            if let Some(text) = first_string(vm, args) {
                vm.with_cx_mut(|cx| self.load(cx, &text));
            }
            return ScriptAsyncResult::Return(NIL);
        }
        if method == live_id!(headings) {
            let json = self.headings_json();
            return ScriptAsyncResult::Return(vm.bx.heap.new_string_from_str(&json).into());
        }
        if method == live_id!(bold) || method == live_id!(italic) {
            vm.with_cx_mut(|cx| {
                let list = self.list(cx);
                self.toggle_emphasis(cx, &list, method == live_id!(bold));
            });
            return ScriptAsyncResult::Return(NIL);
        }
        if method == live_id!(set_style) {
            if let Some(style) = first_string(vm, args).and_then(|id| Style::from_id(&id)) {
                vm.with_cx_mut(|cx| {
                    let list = self.list(cx);
                    self.set_style(cx, &list, style);
                    self.want_frame(cx);
                });
            }
            return ScriptAsyncResult::Return(NIL);
        }
        if method == live_id!(go_to_line) {
            if let Some(line) = first_value(vm, args).as_number() {
                vm.with_cx_mut(|cx| {
                    let list = self.list(cx);
                    self.go_to_line(cx, &list, line.max(0.0) as usize);
                    self.want_frame(cx);
                });
            }
            return ScriptAsyncResult::Return(NIL);
        }
        if method == live_id!(find) {
            // find(words, step): Find's match chosen (`draft::pick_found`), selected and
            // shown while the keyboard stays in the find field; says {"index", "count"}.
            let words = first_string(vm, args).unwrap_or_default();
            let step = nth_value(vm, args, 1).as_number().unwrap_or(0.0) as i32;
            let said = vm.with_cx_mut(|cx| {
                let list = self.list(cx);
                let (index, count) = self.find(cx, &list, &words, step);
                format!("{{\"index\":{index},\"count\":{count}}}")
            });
            return ScriptAsyncResult::Return(vm.bx.heap.new_string_from_str(&said).into());
        }
        if method == live_id!(find_done) {
            // The find bar closed: the keyboard is back on the page, the match selected.
            vm.with_cx_mut(|cx| {
                self.active = true;
                let (anchor, caret) = (self.anchor, self.caret);
                self.focus_at(cx, anchor, caret);
                self.want_frame(cx);
            });
            return ScriptAsyncResult::Return(NIL);
        }
        if method == live_id!(focus) {
            vm.with_cx_mut(|cx| {
                self.active = true;
                self.pending_focus = true;
                self.focus_misses = 0;
                self.want_frame(cx);
                self.view.redraw(cx);
            });
            return ScriptAsyncResult::Return(NIL);
        }
        if method == live_id!(set_active) {
            let active = first_value(vm, args).as_bool().unwrap_or(true);
            vm.with_cx_mut(|cx| self.set_active(cx, active));
            return ScriptAsyncResult::Return(NIL);
        }
        if method == live_id!(delete_selected) || method == live_id!(paste_matching) {
            vm.with_cx_mut(|cx| {
                let list = self.list(cx);
                if method == live_id!(delete_selected) {
                    self.delete_selected(cx, &list);
                } else if let Some(text) = pasteboard_text() {
                    self.paste_matching(cx, &list, &text);
                }
                self.want_frame(cx);
            });
            return ScriptAsyncResult::Return(NIL);
        }
        if method == live_id!(replace_found) {
            // replace_found(words, with, all): says {"index", "count", "replaced"}.
            let words = first_string(vm, args).unwrap_or_default();
            let with = nth_string(vm, args, 1).unwrap_or_default();
            let all = nth_value(vm, args, 2).as_bool().unwrap_or(false);
            let said = vm.with_cx_mut(|cx| {
                let list = self.list(cx);
                let (index, count, replaced) = self.replace_found(cx, &list, &words, &with, all);
                self.want_frame(cx);
                format!("{{\"index\":{index},\"count\":{count},\"replaced\":{replaced}}}")
            });
            return ScriptAsyncResult::Return(vm.bx.heap.new_string_from_str(&said).into());
        }
        if method == live_id!(change_case) {
            if let Some(case) = first_string(vm, args).and_then(|id| draft::Case::from_id(&id)) {
                vm.with_cx_mut(|cx| {
                    let list = self.list(cx);
                    self.change_case(cx, &list, case);
                    self.want_frame(cx);
                });
            }
            return ScriptAsyncResult::Return(NIL);
        }
        if method == live_id!(cut) || method == live_id!(copy) || method == live_id!(paste) {
            vm.with_cx_mut(|cx| {
                let list = self.list(cx);
                if method == live_id!(cut) {
                    self.cut(cx, &list);
                } else if method == live_id!(copy) {
                    self.copy(cx, &list);
                } else if let Some(text) = pasteboard_text() {
                    self.insert_text(cx, &list, &text);
                }
                self.want_frame(cx);
            });
            return ScriptAsyncResult::Return(NIL);
        }
        self.view.script_call(vm, method, args)
    }

    fn text(&self) -> String {
        self.text.clone()
    }

    fn set_text(&mut self, cx: &mut Cx, text: &str) {
        self.load(cx, text);
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if let Some(owner) = cx.script_ref_vm_id(&self.source) {
            if widget_async::current_splash_vm_id(cx) != owner {
                return widget_async::with_isolate(cx, owner, |cx| self.handle_event(cx, event, scope));
            }
        }
        if !self.loaded {
            self.load_lines("");
        }
        let list = self.list(cx);
        if !self.active {
            match event {
                Event::MouseDown(_)
                | Event::MouseUp(_)
                | Event::MouseMove(_)
                | Event::Scroll(_)
                | Event::KeyDown(_)
                | Event::KeyUp(_)
                | Event::TextInput(_)
                | Event::TextCopy(_)
                | Event::TextCut(_)
                | Event::TouchUpdate(_)
                | Event::LongPress(_) => {}
                _ => self.view.handle_event(cx, event, scope),
            }
            self.publish(cx);
            return;
        }
        if self.frame.is_some_and(|frame| frame.is_event(event).is_some()) {
            self.on_frame(cx, &list, scope);
        }
        // Where a press began says what its release means.
        match event {
            Event::MouseDown(e) if e.button.is_primary() => {
                let outside = !list.area().rect(cx).contains(e.abs);
                if self.foreign_focus(cx, &list) {
                    // A press on the page closes an open menu first. Off the page,
                    // another field may take the keyboard on this same press.
                    let focus = cx.key_focus();
                    let on_focus = focus.is_valid(cx) && focus.rect(cx).contains(e.abs);
                    self.press = if outside { Press::Beside(focus, on_focus) } else { Press::BehindOther };
                    self.publish(cx);
                    return;
                }
                self.press = if outside && self.focused_row(cx, &list).is_some() { Press::Away } else { Press::Page };
            }
            Event::MouseDown(e) => {
                self.press = Press::None;
                self.right_press(cx, &list, e.abs);
            }
            Event::MouseUp(_) => match std::mem::take(&mut self.press) {
                // The release of the press that closed the page's own menu.
                Press::None => {
                    if self.right_caret {
                        self.view.redraw(cx);
                    }
                    self.publish(cx);
                    return;
                }
                // The row kept the keyboard, unless what was pressed took it.
                Press::Away => {
                    if self.focused_row(cx, &list).is_none() && !self.foreign_focus(cx, &list) {
                        self.pending_focus = true;
                        self.focus_misses = 0;
                        self.view.redraw(cx);
                    }
                    self.publish(cx);
                    return;
                }
                // A press that gave the keyboard to another field leaves it there; one that only
                // closed a menu, or took it nowhere, gives it back to the page.
                Press::Beside(had, pressed_had) => {
                    let now = cx.key_focus();
                    // Another field can take focus on mouse-down, before this editor
                    // receives the event. A press on that field must keep its focus.
                    let taken = !now.is_empty() && now != self.focus_area && (now != had || pressed_had);
                    if !taken && self.focused_row(cx, &list).is_none() {
                        self.pending_focus = true;
                        self.focus_misses = 0;
                        self.view.redraw(cx);
                    }
                    self.publish(cx);
                    return;
                }
                Press::BehindOther => {
                    if self.focused_row(cx, &list).is_none() {
                        self.pending_focus = true;
                        self.focus_misses = 0;
                        self.view.redraw(cx);
                    }
                    self.publish(cx);
                    return;
                }
                Press::Page => {}
            },
            _ => {}
        }
        // A key waits for the row it belongs to. The caret's row may also have
        // scrolled off the page: typing goes back to it.
        let modifier_only = matches!(event, Event::KeyDown(key) if matches!(key.key_code,
            KeyCode::Shift | KeyCode::Control | KeyCode::Alt | KeyCode::Logo | KeyCode::Capslock));
        if matches!(event, Event::KeyDown(_) | Event::TextInput(_)) && !modifier_only {
            let lost = !self.pending_focus
                && self.focused_row(cx, &list).is_none()
                && !self.focus_area.is_empty()
                && cx.has_key_focus(self.focus_area);
            if lost {
                self.pending_focus = true;
                self.focus_misses = 0;
                self.reveal = 3;
                self.view.redraw(cx);
            }
            if self.pending_focus {
                match event {
                    Event::KeyDown(key) => self.held.push(Held::Key(*key)),
                    Event::TextInput(text) => self.held.push(Held::Text(text.clone())),
                    _ => {}
                }
                self.want_frame(cx);
                self.view.redraw(cx);
                return;
            }
        }
        if let Event::KeyDown(key) = event {
            if !modifier_only && self.handle_key(cx, &list, key) {
                self.track(cx, &list, true);
                self.publish(cx);
                return;
            }
        }
        // Text over a selection across rows: replaced where it is, the page staying put.
        if let Some(selection) = self.selection.selection {
            if self.focused_row(cx, &list).is_some() {
                match event {
                    Event::TextCopy(copy) => {
                        *copy.response.borrow_mut() = Some(self.selected_text(selection));
                        return;
                    }
                    Event::TextCut(copy) => {
                        *copy.response.borrow_mut() = Some(self.selected_text(selection));
                        if let Some(at) = self.replace_selection(cx, selection, "", false) {
                            self.commit(cx, EditKind::Other);
                            self.collapse_at(cx, at);
                        }
                        self.publish(cx);
                        return;
                    }
                    Event::TextInput(input) if input.composition.is_none() => {
                        if let Some(at) = self.replace_selection(cx, selection, &input.input, false) {
                            self.commit(cx, EditKind::Other);
                            self.collapse_at(cx, at);
                        }
                        self.publish(cx);
                        return;
                    }
                    _ => {}
                }
            }
        }
        let by_key = matches!(event, Event::KeyDown(_) | Event::TextInput(_));
        match self.selection.handle_event(cx, event, &list, &mut self.document, &mut self.spare_history) {
            SelectionUpdate::Changed => {
                // Rinx replaced the selection itself (an IME's composition).
                let live: HashSet<&String> = self.document.blocks.iter().map(|b| &b.id).collect();
                self.meta.retain(|id, _| live.contains(id));
                for index in 0..self.document.blocks.len() {
                    if self.meta.get(&self.document.blocks[index].id).is_none() {
                        let id = self.document.blocks[index].id.clone();
                        self.meta.insert(id, Meta { style: Style::Body, marker: String::new(), source: String::new() });
                    }
                }
                let joined = self.document.blocks.iter().map(|b| b.text.clone()).collect::<Vec<_>>();
                for (index, _) in joined.iter().enumerate() {
                    self.refresh_source(index);
                }
                self.commit(cx, EditKind::Other);
                self.publish(cx);
                return;
            }
            SelectionUpdate::Handled => {
                self.track(cx, &list, by_key);
                self.publish(cx);
                return;
            }
            SelectionUpdate::Pass => {}
        }
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        if matches!(event, Event::MouseDown(_) | Event::MouseUp(_)) {
            self.selection.after_event(cx, &list, &self.document);
        }
        // A drag across rows ends with the caret where it stopped.
        if matches!(event, Event::MouseUp(_)) {
            if let Some(selection) = self.selection.selection {
                if selection.anchor.block != selection.cursor.block {
                    if let Some(input) = self.row_input(cx, &list, selection.cursor.block) {
                        if !input.has_focus(cx) {
                            input.take_key_focus(cx);
                        }
                    }
                }
            }
        }
        // A press beside the text, in the page's margins or under the last line.
        match event {
            Event::MouseDown(e) if e.button.is_primary() => {
                let viewport = list.area().rect(cx);
                let beside_bar = e.abs.x < viewport.pos.x + viewport.size.x - 14.0;
                let on_text = self.last_drawn >= self.first_drawn
                    && (self.first_drawn..=self.last_drawn).any(|index| {
                        self.row_input(cx, &list, index).is_some_and(|input| input.area().rect(cx).contains(e.abs))
                    });
                self.margin_press = (viewport.contains(e.abs) && beside_bar && !on_text).then_some(e.abs);
                self.refocus_on_up = !on_text && self.margin_press.is_none() && self.focused_row(cx, &list).is_some();
            }
            Event::MouseUp(e) => {
                if std::mem::take(&mut self.refocus_on_up) {
                    self.pending_focus = true;
                    self.focus_misses = 0;
                    self.view.redraw(cx);
                }
                if let Some(down) = self.margin_press.take() {
                    if (e.abs - down).length() < 6.0 {
                        let mut best: Option<(f64, usize, DVec2)> = None;
                        for index in self.first_drawn..=self.last_drawn {
                            let Some(input) = self.row_input(cx, &list, index) else { continue };
                            let area = input.area();
                            if !area.is_valid(cx) {
                                continue;
                            }
                            let r = area.rect(cx);
                            let distance = (r.pos.y - e.abs.y).max(0.0) + (e.abs.y - r.pos.y - r.size.y).max(0.0);
                            let point = dvec2(
                                e.abs.x.clamp(r.pos.x, r.pos.x + r.size.x),
                                e.abs.y.clamp(r.pos.y + 1.0, r.pos.y + r.size.y - 1.0),
                            );
                            if best.map_or(true, |b| distance < b.0) {
                                best = Some((distance, index, point));
                            }
                        }
                        if let Some((_, index, point)) = best {
                            let at = self.row_input(cx, &list, index).and_then(|input| input.cursor_at(cx, point));
                            let byte = at.map_or(self.document.blocks[index].text.len(), |c| c.index);
                            self.collapse_at(cx, Position { block: index, byte });
                        }
                    }
                }
            }
            Event::MouseMove(e) => {
                let viewport = list.area().rect(cx);
                let over = viewport.contains(e.abs) && e.abs.x < viewport.pos.x + viewport.size.x - 14.0;
                if over {
                    cx.set_cursor(MouseCursor::Text);
                    self.text_cursor = true;
                } else if self.text_cursor {
                    self.text_cursor = false;
                    cx.set_cursor(MouseCursor::Default);
                }
            }
            _ => {}
        }
        for (index, row) in list.items_with_actions(&actions) {
            let input = row.article_rich_input(cx, ids!(rich));
            if input.is_empty() {
                continue;
            }
            if input.changed(&actions).is_some() {
                self.input_changed(cx, &list, index, &input);
            }
            if let Some(key) = input.key_down_unhandled(&actions) {
                self.arrow_out(cx, &list, index, &input, &key);
            }
        }
        if matches!(event, Event::KeyDown(_) | Event::TextInput(_) | Event::MouseDown(_) | Event::MouseUp(_) | Event::TextCut(_)) {
            self.track(cx, &list, by_key);
        }
        self.publish(cx);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if let Some(owner) = cx.script_ref_vm_id(&self.source) {
            if widget_async::current_splash_vm_id(cx) != owner {
                return widget_async::with_isolate(cx, owner, |cx| self.draw_walk(cx, scope, walk));
            }
        }
        if !self.loaded {
            self.load_lines("");
        }
        if self.numbers_dirty {
            self.recount();
        }
        let page = cx.turtle().inner_rect().size.x;
        let page = if page.is_nan() { 612.0 } else { page };
        let inset = ((page - LINE_WIDTH) / 2.0).clamp(28.0, 72.0);
        let count = self.document.blocks.len();
        let mut jump = None;
        let paper_top = self.paper_top(cx);
        if let Some(mut paper) = self.view.view(cx, ids!(paper)).borrow_mut() {
            paper.walk.margin.top = paper_top;
        }
        // The menu a right click opened is done with the keyboard: the caret's row has it again.
        if self.right_caret {
            let list = self.list(cx);
            if !self.foreign_focus(cx, &list) {
                self.right_caret = false;
                self.pending_focus = true;
                self.focus_misses = 0;
            }
        }
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            let portal = step.as_portal_list();
            let Some(mut list) = portal.borrow_mut() else { continue };
            if list.first_id() >= count {
                list.set_first_id_and_scroll(count.saturating_sub(1), 0.0);
            }
            list.set_item_range(cx, 0, count);
            let mut first = usize::MAX;
            let mut last = 0;
            let mut took_focus = false;
            let mut focus_drawn = false;
            while let Some(index) = list.next_visible_item(cx) {
                let Some(block) = self.document.blocks.get(index) else { continue };
                first = first.min(index);
                last = last.max(index);
                let style = self.meta.get(&block.id).map_or(Style::Body, |m| m.style);
                let row = list.item(cx, index, template_of(style));
                let input = row.article_rich_input(cx, ids!(rich));
                let marker = self.meta.get(&block.id).map_or("", |m| m.marker.as_str());
                let level = if style.is_list() { draft::list_level(marker) as f64 } else { 0.0 };
                // Where the row's text starts: a list's mark sits a cell in, a quote in
                // the middle of the line, as the document has them.
                let mut left = inset;
                if style.is_list() {
                    left += 18.0 * (level + 1.0);
                }
                if style == Style::Quote {
                    let room = (page - inset * 2.0).max(1.0) as f32;
                    let laid = self.measure.layout(cx, 0.0, 0.0, Some(room), true, Align::default(), &block.text);
                    left += ((room - laid.size_in_lpxs.width).max(0.0) / 2.0) as f64;
                }
                let next_is_code = self.document.blocks.get(index + 1).and_then(|b| self.meta.get(&b.id)).is_some_and(|m| m.style == Style::Code);
                let gap = if (style == Style::Code && next_is_code) || (style == Style::Body && block.text.is_empty()) { 0.0 } else { PARAGRAPH_GAP };
                let gap = if block.marks.is_empty() { gap } else { (gap - self.marked_leading(cx, style)).max(0.0) };
                let padding = Inset {
                    left,
                    right: inset,
                    top: space_above(style) + if index == 0 { PAGE_TOP + self.page_gap } else { 0.0 },
                    bottom: gap + if index + 1 == count { PAGE_BOTTOM } else { 0.0 },
                };
                if let Some(mut view) = row.as_view().borrow_mut() {
                    if view.layout.padding != padding {
                        view.layout.padding = padding;
                    }
                }
                if style.is_list() {
                    let number = self.numbers.get(index).copied().unwrap_or(0);
                    row.view(cx, ids!(dot)).set_visible(cx, style == Style::Bullet);
                    let num = row.label(cx, ids!(num));
                    num.set_visible(cx, style == Style::Number);
                    if style == Style::Number {
                        let shown = format!("{number}.");
                        if num.text() != shown {
                            num.set_text(cx, &shown);
                        }
                    }
                    if let Some(mut mark) = row.view(cx, ids!(mark)).borrow_mut() {
                        let width = if number >= 10 { 26.0 } else { 18.0 };
                        mark.walk.width = Size::Fixed(width);
                    }
                }
                input.set_block(cx, block, font_size_of(style));
                input.set_empty_text(cx, if count == 1 && block.text.is_empty() && style == Style::Body { self.empty_text.clone() } else { String::new() });
                self.selection.apply_to_input(cx, index, &input, &self.document);
                // Only the caret's row has a selection of its own. A row keeps one when the
                // keyboard moves on, and a replaced selection's first row keeps it on its new text.
                if self.selection.selection.is_none() && index != self.caret.block && !input.has_focus(cx) && input.has_selection() {
                    let cursor = input.selection().cursor;
                    input.set_selection(cx, Selection { anchor: cursor, cursor });
                }
                row.draw_all(cx, scope);
                self.selection.after_draw(cx, index, &input);
                if input.has_focus(cx) {
                    self.focus_area = input.area();
                }
                if self.pending_focus && self.active && self.caret.block == index {
                    if self.selection.selection.is_none() {
                        let anchor = if self.anchor.block == index { self.anchor.byte } else { self.caret.byte };
                        input.set_selection(cx, Selection {
                            anchor: Cursor { index: anchor, prefer_next_row: false },
                            cursor: Cursor { index: self.caret.byte, prefer_next_row: false },
                        });
                    }
                    input.take_key_focus(cx);
                    self.focus_area = input.area();
                    took_focus = true;
                    focus_drawn = true;
                }
            }
            self.first_drawn = if first == usize::MAX { 0 } else { first };
            self.last_drawn = last;
            if took_focus {
                self.pending_focus = false;
                self.frame = Some(cx.new_next_frame());
            } else if self.pending_focus && self.active && !focus_drawn {
                // The row isn't on the page yet: bring it, a few times at most.
                self.focus_misses += 1;
                if self.focus_misses > 4 {
                    self.pending_focus = false;
                    self.held.clear();
                } else {
                    let block = self.caret.block.min(count.saturating_sub(1));
                    let height = list.area().rect(cx).size.y;
                    jump = Some((block, if block < self.first_drawn { 16.0 } else { (height - 72.0).max(0.0) }));
                }
            }
        }
        // Not while the list draws: it ends by looking for the row it started from.
        if let Some((block, at)) = jump {
            self.list(cx).set_first_id_and_scroll(block, at);
            self.want_frame(cx);
        }
        // A Mac's Edit menu asks the field with the keyboard what it can do, as a
        // text field tells it (`HostedEdit`). The page's rows don't, so the page
        // does, for its own selection and history: Cut and Copy grey with nothing
        // selected, and Undo with nothing to undo, as TextEdit's do.
        if !self.focus_area.is_empty() && cx.has_key_focus(self.focus_area) {
            cx.publish_hosted_edit(makepad_widgets::makepad_platform::ime::HostedEdit {
                field: true,
                selected: self.anchor != self.caret,
                has_text: !self.text.is_empty(),
                writable: true,
                can_undo: !self.undo.is_empty(),
                can_redo: !self.redo.is_empty(),
            });
        }
        DrawStep::done()
    }
}

// ---- the read-only page ------------------------------------------------------

thread_local! {
    /// The marks a scroller's page drew, in the order it drew them.
    static MARKS: RefCell<Vec<(usize, f64)>> = const { RefCell::new(Vec::new()) };
}

/// What a `DocMark{end: true}` is known by.
const END_MARK: usize = usize::MAX;

/// A heading a `DocumentScroller` can scroll to: `mark` is its number. With `end: true`
/// it says where what's drawn so far ends, when there's more to come.
#[derive(Script, ScriptHook, Widget)]
pub struct DocMark {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    mark: f64,
    #[live]
    end: bool,
}

impl Widget for DocMark {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let step = self.view.draw_walk(cx, scope, walk);
        if !step.is_step() {
            let y = self.view.area().rect(cx).pos.y;
            let mark = if self.end { END_MARK } else { self.mark as usize };
            MARKS.with(|marks| marks.borrow_mut().push((mark, y)));
        }
        step
    }
}

/// A scrolling page that knows where its `DocMark`s are: `scroll_to_mark(k)` and
/// `reveal_mark(k)`, and `on_mark(k)` when scrolling reaches another one. `DocMark{end: true}`
/// is where the page ends: `on_near_end` asks for the rest of a long page as scrolling comes
/// near it, and over the last screenful the page's last marks are reached in turn.
#[derive(Script, ScriptHook, Widget)]
pub struct DocumentScroller {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    on_mark: Option<ScriptFnRef>,
    #[live]
    on_near_end: Option<ScriptFnRef>,
    /// Content heights of the marks.
    #[rust]
    marks: Vec<(usize, f64)>,
    /// Content height of the end mark, and the one already asked about.
    #[rust]
    end_y: Option<f64>,
    #[rust]
    asked_end: Option<f64>,
    #[rust]
    wanted: Option<usize>,
    #[rust]
    wanted_reveal: Option<usize>,
    /// The mark asked for stays the current one until the person scrolls.
    #[rust]
    held_mark: bool,
    #[rust]
    current: Option<usize>,
    #[rust]
    last_y: f64,
    #[rust]
    frame: Option<NextFrame>,
}

impl DocumentScroller {
    fn scroll_to(&mut self, cx: &mut Cx, k: usize) -> bool {
        let Some((_, y)) = self.marks.iter().find(|(m, _)| *m == k).copied() else { return false };
        let target = (y - 16.0).max(0.0);
        self.view.set_scroll_pos(cx, dvec2(0.0, target));
        self.last_y = target;
        self.held_mark = true;
        self.current = Some(k);
        self.view.redraw(cx);
        true
    }

    /// Only as far as it takes to have the mark's row in sight. Going up, a row the first
    /// screenful holds takes the page to its top, so what's above it shows too, as a Mac's
    /// source list keeps its sections in view.
    fn reveal(&mut self, cx: &mut Cx, k: usize) -> bool {
        const ROW: f64 = 36.0;
        let Some((_, y)) = self.marks.iter().find(|(m, _)| *m == k).copied() else { return false };
        let seen = self.view.area().rect(cx).size.y;
        // Not laid out yet (hidden, or not drawn): when it is.
        if seen <= 0.0 {
            return false;
        }
        let now = self.view.scroll_pos().y;
        let target = if y < now {
            if y + ROW <= seen { 0.0 } else { y }
        } else if y + ROW > now + seen {
            y + ROW - seen
        } else {
            return true;
        };
        self.view.set_scroll_pos(cx, dvec2(0.0, target.max(0.0)));
        self.last_y = target.max(0.0);
        self.view.redraw(cx);
        true
    }

    fn ask_for_more(&mut self, cx: &mut Cx, y: f64) {
        let Some(end) = self.end_y else { return };
        let seen = self.view.area().rect(cx).size.y;
        if end - (y + seen) > 900.0 || self.asked_end == Some(end) {
            return;
        }
        self.asked_end = Some(end);
        if let Some(handler) = self.on_near_end.clone() {
            cx.widget_to_script_call(self.widget_uid(), NIL, self.source.clone(), handler, &[]);
        }
    }

    /// Where a mark counts as reached: a little under the view's top, going down to its
    /// bottom over the last screenful, so the marks that can't scroll up that far get their
    /// turn. The `DocMark{end: true}` says where the page ends.
    fn reading_line(&self, cx: &Cx, y: f64) -> f64 {
        const LINE: f64 = 120.0;
        let line = y + LINE;
        let Some(end) = self.end_y else { return line };
        let seen = self.view.area().rect(cx).size.y;
        let furthest = (end - seen).max(0.0);
        let span = seen.min(furthest);
        if span <= 0.0 {
            return line;
        }
        let left = (furthest - y).clamp(0.0, span);
        line + (seen - LINE).max(0.0) * (1.0 - left / span)
    }

    fn notify(&mut self, cx: &mut Cx) {
        let y = self.view.scroll_pos().y;
        let line = self.reading_line(cx, y);
        let reached = self
            .marks
            .iter()
            .filter(|(_, at)| *at <= line)
            .last()
            .or(self.marks.first())
            .map(|(k, _)| *k);
        if reached == self.current {
            return;
        }
        self.current = reached;
        if let (Some(handler), Some(k)) = (self.on_mark.clone(), reached) {
            cx.widget_to_script_call(self.widget_uid(), NIL, self.source.clone(), handler, &[ScriptValue::from(k as f64)]);
        }
    }
}

impl Widget for DocumentScroller {
    fn script_call(&mut self, vm: &mut ScriptVm, method: LiveId, args: ScriptValue) -> ScriptAsyncResult {
        if method == live_id!(scroll_to_mark) {
            if let Some(k) = first_value(vm, args).as_number() {
                let k = k.max(0.0) as usize;
                vm.with_cx_mut(|cx| {
                    if !self.scroll_to(cx, k) {
                        // Not drawn yet: when the page next is.
                        self.wanted = Some(k);
                        self.view.redraw(cx);
                    }
                });
            }
            return ScriptAsyncResult::Return(NIL);
        }
        if method == live_id!(reveal_mark) {
            if let Some(k) = first_value(vm, args).as_number() {
                let k = k.max(0.0) as usize;
                vm.with_cx_mut(|cx| {
                    if !self.reveal(cx, k) {
                        self.wanted_reveal = Some(k);
                        self.view.redraw(cx);
                    }
                });
            }
            return ScriptAsyncResult::Return(NIL);
        }
        if method == live_id!(scroll_to_top) {
            vm.with_cx_mut(|cx| {
                self.view.set_scroll_pos(cx, dvec2(0.0, 0.0));
                self.last_y = 0.0;
                self.held_mark = false;
                self.current = None;
                self.wanted = None;
                self.view.redraw(cx);
            });
            return ScriptAsyncResult::Return(NIL);
        }
        self.view.script_call(vm, method, args)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if let Some(owner) = cx.script_ref_vm_id(&self.source) {
            if widget_async::current_splash_vm_id(cx) != owner {
                return widget_async::with_isolate(cx, owner, |cx| self.handle_event(cx, event, scope));
            }
        }
        self.view.handle_event(cx, event, scope);
        if self.frame.is_some_and(|frame| frame.is_event(event).is_some()) {
            self.frame = None;
            if let Some(k) = self.wanted.take() {
                self.scroll_to(cx, k);
            }
            if let Some(k) = self.wanted_reveal.take() {
                self.reveal(cx, k);
            }
        }
        if matches!(event, Event::Scroll(_)) {
            self.held_mark = false;
        }
        let y = self.view.scroll_pos().y;
        if y != self.last_y {
            self.last_y = y;
            if !self.held_mark {
                self.notify(cx);
            }
            self.ask_for_more(cx, y);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if let Some(owner) = cx.script_ref_vm_id(&self.source) {
            if widget_async::current_splash_vm_id(cx) != owner {
                return widget_async::with_isolate(cx, owner, |cx| self.draw_walk(cx, scope, walk));
            }
        }
        MARKS.with(|marks| marks.borrow_mut().clear());
        while self.view.draw_walk(cx, scope, walk).is_step() {}
        let drawn = MARKS.with(|marks| std::mem::take(&mut *marks.borrow_mut()));
        if !drawn.is_empty() {
            let top = self.view.area().rect(cx).pos.y;
            let y = self.view.scroll_pos().y;
            self.end_y = drawn.iter().find(|(k, _)| *k == END_MARK).map(|(_, at)| at - top + y);
            self.marks = drawn.into_iter().filter(|(k, _)| *k != END_MARK).map(|(k, at)| (k, at - top + y)).collect();
            if (self.wanted.is_some() || self.wanted_reveal.is_some()) && self.frame.is_none() {
                self.frame = Some(cx.new_next_frame());
            }
        }
        DrawStep::done()
    }
}
