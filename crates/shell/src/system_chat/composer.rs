//! The prompt of a chat pane (the system chat and "Ask <app>"): the text
//! being typed, fed the way makepad's `TextInput` widget is fed.
//!
//! - **Characters come only from text input** (`Event::TextInput`): a
//!   desktop's typed characters, a paste, and every edit of a phone's input
//!   method. A key press never adds one: the platform sends the character
//!   as text input as well, and adding it on the key too typed every
//!   character twice.
//! - **Keys** are the pane's commands only: Return sends, Backspace
//!   deletes, Escape closes (see `super::key`, `crate::app_chat::key`).
//! - **Android's input method** edits a whole editor state: before an edit
//!   it asks the focused editor for its text, selection and composition
//!   (`Event::TextInputStateQuery`, answered with [`Composer::state`]), and
//!   then sends the new state (`TextInputEvent::full_state_sync`), which
//!   replaces the text. Without the answer the platform drops the edit.
//! - **Composition** (a word being composed, `replace_last`) replaces the
//!   previous preview until it is committed, as the widget does.
//! - **Lines**: Return sends; Shift+Return breaks the line
//!   ([`Composer::newline`]); a paste keeps its line breaks. A line break an
//!   input method types (a phone keyboard's Enter) still sends: it is taken
//!   out and asks to send ([`Composer::take_submit`]), and the breaks the
//!   prompt already held stay.
//! - **The caret and the selection**: arrows, Option for words, Command
//!   for the line's (and Up/Down the text's) ends, Home and End, Shift to
//!   select, Command+A; a click places the caret, a drag or Shift+click
//!   selects, a double-click takes a word ([`Composer::motion`],
//!   [`Composer::word_at`]). Typing, a paste and a composition go in at the
//!   caret over the selection; Backspace and Delete take the selection or
//!   the character beside the caret; copy and cut take the selection
//!   ([`Composer::copy`]).

use makepad_widgets::makepad_platform::event::{CharOffset, FullTextState, TextInputEvent};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Composer {
    text: String,
    /// The caret, in characters.
    cursor: usize,
    /// The selection's other end, in characters, while one is made.
    anchor: Option<usize>,
    /// The characters (not bytes) being composed, if a word is.
    composition: Option<std::ops::Range<usize>>,
    /// A line break was typed: send.
    submit: bool,
    /// Shift+Return just broke the line: a platform that also types that
    /// break as text input has it dropped once.
    key_break: bool,
}

/// Where a caret moves ([`Composer::motion`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

/// A caret move from the keyboard: its direction, whether it selects
/// (Shift), goes by word (Option/Alt) or to the line's or the text's end
/// (Command).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Motion {
    pub dir: Dir,
    pub select: bool,
    pub word: bool,
    pub line: bool,
}

/// One drawn line of the prompt: where it starts in the text (characters)
/// and the x of each of its character boundaries (one more than its
/// characters), for Up and Down and for a click ([`Composer::index_at`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineLayout {
    pub start: usize,
    pub xs: Vec<f64>,
}

impl LineLayout {
    fn len(&self) -> usize {
        self.xs.len().saturating_sub(1)
    }
    /// The boundary nearest `x` on this line.
    fn nearest(&self, x: f64) -> usize {
        let mut best = 0;
        for (i, bx) in self.xs.iter().enumerate() {
            if (bx - x).abs() < (self.xs[best] - x).abs() {
                best = i;
            }
        }
        self.start + best
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Composer {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The caret, in characters.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The selection, in characters (empty when there is none).
    pub fn selection(&self) -> std::ops::Range<usize> {
        match self.anchor {
            Some(a) if a != self.cursor => a.min(self.cursor)..a.max(self.cursor),
            _ => self.cursor..self.cursor,
        }
    }

    /// The selected text ("" without a selection).
    pub fn selected_text(&self) -> String {
        let r = self.selection();
        self.text.chars().skip(r.start).take(r.len()).collect()
    }

    /// Empty the prompt (after Send) and give back what it held.
    pub fn take(&mut self) -> String {
        self.composition = None;
        self.cursor = 0;
        self.anchor = None;
        std::mem::take(&mut self.text)
    }

    pub fn clear(&mut self) {
        self.take();
    }

    fn chars(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, chars: usize) -> usize {
        CharOffset(chars).to_byte_index(&self.text)
    }

    fn char_at(&self, i: usize) -> Option<char> {
        self.text.chars().nth(i)
    }

    /// Put `with` in place of the characters `range`; the caret after it.
    fn replace_chars(&mut self, range: std::ops::Range<usize>, with: &str) {
        let (a, b) = (self.byte(range.start), self.byte(range.end));
        self.text.replace_range(a..b, with);
        self.cursor = range.start + with.chars().count();
        self.anchor = None;
    }

    /// Type `with` at the caret, over the selection.
    fn insert(&mut self, with: &str) {
        let at = self.selection();
        self.replace_chars(at, with);
    }

    /// Backspace: the selection, else the character before the caret.
    pub fn backspace(&mut self) -> bool {
        self.composition = None;
        let r = self.selection();
        if !r.is_empty() {
            self.replace_chars(r, "");
            return true;
        }
        if self.cursor == 0 {
            return false;
        }
        self.replace_chars(self.cursor - 1..self.cursor, "");
        true
    }

    /// Delete: the selection, else the character after the caret.
    pub fn delete_forward(&mut self) -> bool {
        self.composition = None;
        let r = self.selection();
        if !r.is_empty() {
            self.replace_chars(r, "");
            return true;
        }
        if self.cursor >= self.chars() {
            return false;
        }
        self.replace_chars(self.cursor..self.cursor + 1, "");
        true
    }

    /// Shift+Return: a line break at the caret.
    pub fn newline(&mut self) {
        self.composition = None;
        self.insert("\n");
        self.key_break = true;
    }

    /// Move the caret to `index`, selecting from where it was when `select`.
    pub fn move_to(&mut self, index: usize, select: bool) {
        let index = index.min(self.chars());
        if select {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = index;
        self.composition = None;
    }

    /// Select from `from` to `to` (the caret at `to`).
    pub fn select(&mut self, from: usize, to: usize) {
        let len = self.chars();
        self.anchor = Some(from.min(len));
        self.cursor = to.min(len);
        self.composition = None;
    }

    pub fn select_all(&mut self) {
        let len = self.chars();
        self.select(0, len);
    }

    /// The word (or the run of spaces, or the one other character) at
    /// `index`: what a double-click selects.
    pub fn word_at(&self, index: usize) -> std::ops::Range<usize> {
        let chars: Vec<char> = self.text.chars().collect();
        let i = index.min(chars.len());
        let at = chars.get(i).or_else(|| i.checked_sub(1).and_then(|j| chars.get(j)));
        let Some(&c) = at else { return i..i };
        let i = if chars.get(i).is_some() { i } else { i - 1 };
        let same = |d: char| if is_word(c) { is_word(d) } else if c.is_whitespace() { d.is_whitespace() && d != '\n' } else { false };
        if !is_word(c) && !c.is_whitespace() {
            return i..i + 1;
        }
        let mut a = i;
        while a > 0 && same(chars[a - 1]) {
            a -= 1;
        }
        let mut b = i + 1;
        while b < chars.len() && same(chars[b]) {
            b += 1;
        }
        a..b
    }

    /// The start and end of the line (between line breaks) the caret is on.
    fn line_bounds(&self, index: usize) -> (usize, usize) {
        let chars: Vec<char> = self.text.chars().collect();
        let mut a = index.min(chars.len());
        while a > 0 && chars[a - 1] != '\n' {
            a -= 1;
        }
        let mut b = index.min(chars.len());
        while b < chars.len() && chars[b] != '\n' {
            b += 1;
        }
        (a, b)
    }

    /// A caret move from the keyboard. Up and Down go by the drawn lines
    /// (`layout`, keeping the caret's x); without it, to the text's ends.
    pub fn motion(&mut self, m: Motion, layout: &[LineLayout]) {
        let len = self.chars();
        let sel = self.selection();
        // Left or Right with a selection and no Shift: to its edge.
        if !m.select && !sel.is_empty() && !m.word && !m.line && matches!(m.dir, Dir::Left | Dir::Right) {
            self.move_to(if m.dir == Dir::Left { sel.start } else { sel.end }, false);
            return;
        }
        let chars: Vec<char> = self.text.chars().collect();
        let c = self.cursor;
        let target = match m.dir {
            Dir::Left if m.line => self.line_bounds(c).0,
            Dir::Right if m.line => self.line_bounds(c).1,
            Dir::Left if m.word => {
                let mut i = c;
                while i > 0 && !is_word(chars[i - 1]) {
                    i -= 1;
                }
                while i > 0 && is_word(chars[i - 1]) {
                    i -= 1;
                }
                i
            }
            Dir::Right if m.word => {
                let mut i = c;
                while i < len && !is_word(chars[i]) {
                    i += 1;
                }
                while i < len && is_word(chars[i]) {
                    i += 1;
                }
                i
            }
            Dir::Left => c.saturating_sub(1),
            Dir::Right => (c + 1).min(len),
            Dir::Home => if m.line { 0 } else { self.line_bounds(c).0 },
            Dir::End => if m.line { len } else { self.line_bounds(c).1 },
            Dir::Up | Dir::Down if m.line => if m.dir == Dir::Up { 0 } else { len },
            Dir::Up | Dir::Down => {
                let here = layout.iter().rposition(|l| l.start <= c);
                match here {
                    Some(i) => {
                        let line = &layout[i];
                        let x = line.xs.get(c - line.start).copied().unwrap_or(0.0);
                        let next = if m.dir == Dir::Up { i.checked_sub(1) } else { Some(i + 1).filter(|j| *j < layout.len()) };
                        match next {
                            Some(j) => layout[j].nearest(x).min(layout[j].start + layout[j].len()),
                            None => if m.dir == Dir::Up { 0 } else { len },
                        }
                    }
                    None => if m.dir == Dir::Up { 0 } else { len },
                }
            }
        };
        self.move_to(target, m.select);
    }

    /// The character boundary under `(line, x)` of a drawn layout: where a
    /// click puts the caret.
    pub fn index_at(layout: &[LineLayout], line: usize, x: f64) -> usize {
        match layout.get(line).or_else(|| layout.last()) {
            Some(l) => l.nearest(x),
            None => 0,
        }
    }

    /// Copy (or cut): the selected text; cut takes it out. `None` without
    /// a selection.
    pub fn copy(&mut self, cut: bool) -> Option<String> {
        let r = self.selection();
        if r.is_empty() {
            return None;
        }
        let text = self.selected_text();
        if cut {
            self.composition = None;
            self.replace_chars(r, "");
        }
        Some(text)
    }

    /// One text-input event. True when the text changed.
    pub fn text_input(&mut self, event: &TextInputEvent) -> bool {
        let is_break = |c: char| c == '\n' || c == '\r';
        // The break Shift+Return already made, typed again as text.
        if std::mem::take(&mut self.key_break) && !event.was_paste && event.full_state_sync.is_none() && !event.input.is_empty() && event.input.chars().all(is_break) {
            return false;
        }
        let before = self.text.clone();
        if event.full_state_sync.is_some() {
            self.apply(event, &event.input);
            // An input method's Enter typed into its editor state sends:
            // the new breaks (by the caret) go, the prompt's own stay.
            let breaks = |s: &str| s.chars().filter(|c| is_break(*c)).count();
            let mut typed = breaks(&self.text).saturating_sub(breaks(&before));
            if typed > 0 && !event.was_paste {
                while typed > 0 {
                    let near = self.cursor.checked_sub(1).filter(|i| self.char_at(*i).is_some_and(is_break));
                    let at = near.or_else(|| self.text.chars().collect::<Vec<_>>().iter().rposition(|c| is_break(*c)));
                    let Some(i) = at else { break };
                    let byte = self.byte(i);
                    self.text.remove(byte);
                    if self.cursor > i {
                        self.cursor -= 1;
                    }
                    typed -= 1;
                }
                self.anchor = None;
                self.composition = None;
                self.submit = true;
            }
        } else if event.was_paste {
            // A paste keeps its lines, as plain line breaks.
            let input = event.input.replace("\r\n", "\n").replace('\r', "\n");
            self.apply(event, &input);
        } else if event.input.contains(is_break) {
            // A typed line break (an input method's Enter) sends; the rest
            // of what came with it is typed.
            let input: String = event.input.chars().filter(|c| !is_break(*c)).collect();
            self.apply(event, &input);
            self.composition = None;
            self.submit = true;
        } else {
            self.apply(event, &event.input);
        }
        self.text != before
    }

    /// A line break was typed since the last call (the caller sends).
    pub fn take_submit(&mut self) -> bool {
        std::mem::take(&mut self.submit)
    }

    fn apply(&mut self, event: &TextInputEvent, input: &str) {
        if let Some(state) = &event.full_state_sync {
            // The input method's whole editor state.
            self.text = state.text.clone();
            let end = self.chars();
            let (a, b) = (state.selection.start.0.min(end), state.selection.end.0.min(end));
            self.cursor = b;
            self.anchor = (a != b).then_some(a);
            self.composition = state.composition.as_ref().map(|c| c.start.0.min(end)..c.end.0.min(end)).filter(|c| c.start < c.end);
            return;
        }
        if let Some((start, end)) = event.replace_range {
            // A replacement of a range (iOS autocorrect, a paste over it).
            let (a, b) = (start.0.min(end.0), start.0.max(end.0));
            let len = self.chars();
            self.replace_chars(a.min(len)..b.min(len), input);
            self.composition = None;
            return;
        }
        let len = input.chars().count();
        match self.composition.clone() {
            // A composition preview replaces the one before it; a commit
            // replaces it for good.
            Some(c) => {
                self.replace_chars(c.clone(), input);
                self.composition = (event.replace_last && len > 0).then(|| c.start..c.start + len);
            }
            None => {
                let start = self.selection().start;
                self.insert(input);
                self.composition = (event.replace_last && len > 0).then(|| start..start + len);
            }
        }
    }

    /// The editor state the input method asks for: the text, the selection
    /// (the caret when there is none), and the composition.
    pub fn state(&self) -> FullTextState {
        let r = match self.anchor {
            Some(a) if a != self.cursor => CharOffset(a)..CharOffset(self.cursor),
            _ => CharOffset(self.cursor)..CharOffset(self.cursor),
        };
        FullTextState { text: self.text.clone(), selection: r, composition: self.composition.clone().map(|c| CharOffset(c.start)..CharOffset(c.end)) }
    }
}

/// The soft keyboard's action key (Enter, shown as Send) sends: the prompt
/// is one line. Next and Previous move between fields instead; a keyboard
/// that names no action (or Done, Go, Search) still means "this is it".
pub fn ime_action_sends(action: makepad_widgets::makepad_platform::event::ImeAction) -> bool {
    use makepad_widgets::makepad_platform::event::ImeAction;
    !matches!(action, ImeAction::Next | ImeAction::Previous)
}

/// A chat pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    App,
    System,
}

/// Which pane the keyboard's action key sends: only one whose prompt holds
/// the key focus. With another field focused (an app's, a host sheet's)
/// the action is that field's, never the chat's.
pub fn ime_target(app_has_focus: bool, system_has_focus: bool) -> Option<Pane> {
    if app_has_focus {
        Some(Pane::App)
    } else if system_has_focus {
        Some(Pane::System)
    } else {
        None
    }
}

/// Which pane plain typed text goes to when no prompt holds the key focus
/// (the pane opened by F8, no press yet): only when no other field holds
/// it either.
pub fn text_target(nothing_focused: bool, app_focused: bool, system_open: bool) -> Option<Pane> {
    if !nothing_focused {
        None
    } else if app_focused {
        Some(Pane::App)
    } else if system_open {
        Some(Pane::System)
    } else {
        None
    }
}

/// A key press in a pane's prompt: what the pane does with it. Characters
/// are text input's (see the module); a printable key is only swallowed so
/// it reaches nothing behind the pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Close,
    Send,
    /// Shift+Return: a line break in the prompt.
    NewLine,
    Backspace,
    /// Forward Delete.
    Delete,
    /// The caret moves (arrows, Home, End), selecting with Shift.
    Move(Motion),
    /// Command+A / Control+A.
    SelectAll,
    /// Command+N / Control+N.
    New,
    /// Command+. / Control+.
    Stop,
    /// A printable key: its character arrives as text input.
    Swallow,
    /// Not the pane's (function keys, other shortcuts).
    Pass,
}

pub fn key(e: &makepad_widgets::KeyEvent) -> Key {
    use makepad_widgets::KeyCode;
    let command_key = e.modifiers.logo || e.modifiers.control;
    match e.key_code {
        KeyCode::Escape => Key::Close,
        KeyCode::ReturnKey if !e.modifiers.shift => Key::Send,
        KeyCode::ReturnKey if !command_key => Key::NewLine,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::KeyA if command_key => Key::SelectAll,
        KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::Home | KeyCode::End => {
            let dir = match e.key_code {
                KeyCode::ArrowLeft => Dir::Left,
                KeyCode::ArrowRight => Dir::Right,
                KeyCode::ArrowUp => Dir::Up,
                KeyCode::ArrowDown => Dir::Down,
                KeyCode::Home => Dir::Home,
                _ => Dir::End,
            };
            Key::Move(Motion { dir, select: e.modifiers.shift, word: e.modifiers.alt, line: e.modifiers.logo })
        }
        KeyCode::KeyN if command_key => Key::New,
        KeyCode::Period if command_key => Key::Stop,
        other if !command_key && other.to_char(e.modifiers.shift).is_some() => Key::Swallow,
        _ => Key::Pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_widgets::{KeyCode, KeyEvent, KeyModifiers};

    fn typed(text: &str) -> TextInputEvent {
        TextInputEvent { input: text.into(), ..Default::default() }
    }

    fn composing(text: &str) -> TextInputEvent {
        TextInputEvent { input: text.into(), replace_last: true, ..Default::default() }
    }

    fn ime_state(text: &str, composition: Option<std::ops::Range<usize>>) -> TextInputEvent {
        let end = CharOffset(text.chars().count());
        TextInputEvent { full_state_sync: Some(FullTextState { text: text.into(), selection: end..end, composition: composition.map(|c| CharOffset(c.start)..CharOffset(c.end)) }), ..Default::default() }
    }

    fn press(key_code: KeyCode) -> KeyEvent {
        KeyEvent { key_code, ..Default::default() }
    }

    /// A desktop sends each character twice: KeyDown, then TextInput. The
    /// prompt holds it once (the bug: "hheelllloo").
    #[test]
    fn a_desktop_keystroke_types_its_character_once() {
        let mut c = Composer::default();
        for (code, ch) in [(KeyCode::KeyH, "h"), (KeyCode::KeyI, "i")] {
            assert_eq!(key(&press(code)), Key::Swallow, "the key is the pane's, its character is not typed");
            c.text_input(&typed(ch));
        }
        assert_eq!(key(&KeyEvent { key_code: KeyCode::Key1, modifiers: KeyModifiers { shift: true, ..Default::default() }, ..Default::default() }), Key::Swallow);
        c.text_input(&typed("!"));
        assert_eq!(c.text(), "hi!");
        assert_eq!(key(&press(KeyCode::Backspace)), Key::Backspace);
        assert!(c.backspace());
        assert_eq!(c.text(), "hi");
        assert_eq!(key(&press(KeyCode::ReturnKey)), Key::Send);
        assert_eq!(c.take(), "hi");
        assert!(c.is_empty());
    }

    #[test]
    fn keys_are_commands_and_function_keys_pass() {
        assert_eq!(key(&press(KeyCode::Escape)), Key::Close);
        assert_eq!(key(&press(KeyCode::F8)), Key::Pass);
        let shifted = |k| KeyEvent { key_code: k, modifiers: KeyModifiers { shift: true, ..Default::default() }, ..Default::default() };
        assert_ne!(key(&shifted(KeyCode::ReturnKey)), Key::Send, "Shift+Return is no Send key");
        let command = |k| KeyEvent { key_code: k, modifiers: KeyModifiers { logo: true, ..Default::default() }, ..Default::default() };
        assert_eq!(key(&command(KeyCode::KeyN)), Key::New);
        assert_eq!(key(&command(KeyCode::Period)), Key::Stop);
        assert_eq!(key(&command(KeyCode::KeyV)), Key::Pass, "paste arrives as text input");
    }

    /// Android's input method: the editor state is asked for, then the new
    /// state replaces the text (composing, committing, deleting).
    #[test]
    fn the_phones_input_method_edits_the_whole_state() {
        let mut c = Composer::default();
        assert_eq!(c.state().text, "");
        assert!(c.text_input(&ime_state("Hel", Some(0..3))));
        assert_eq!(c.state().composition, Some(CharOffset(0)..CharOffset(3)));
        c.text_input(&ime_state("Hello", Some(0..5)));
        c.text_input(&ime_state("Hello ", None));
        assert_eq!(c.text(), "Hello ");
        let state = c.state();
        assert_eq!((state.selection.clone(), state.composition), (CharOffset(6)..CharOffset(6), None), "the caret at the end");
        c.text_input(&ime_state("Hello wö", Some(6..8)));
        assert_eq!(c.state().selection, CharOffset(8)..CharOffset(8), "characters, not bytes");
        c.text_input(&ime_state("Hello w", Some(6..7)));
        assert_eq!(c.text(), "Hello w");
        assert!(!c.take_submit());
    }

    #[test]
    fn a_composition_replaces_its_preview_until_committed() {
        let mut c = Composer::default();
        c.text_input(&typed("I "));
        c.text_input(&composing("ni"));
        c.text_input(&composing("nih"));
        assert_eq!(c.text(), "I nih");
        c.text_input(&typed("你好"));
        assert_eq!(c.text(), "I 你好");
        c.text_input(&typed("!"));
        assert_eq!(c.text(), "I 你好!");
        // A cancelled composition leaves nothing.
        c.text_input(&composing("x"));
        c.text_input(&composing(""));
        assert_eq!(c.text(), "I 你好!");
    }

    #[test]
    fn an_input_methods_enter_sends_and_is_not_typed() {
        let mut c = Composer::default();
        c.text_input(&ime_state("ask", None));
        c.text_input(&ime_state("ask\n", None));
        assert_eq!(c.text(), "ask");
        assert!(c.take_submit());
        assert!(!c.take_submit());
        let mut c = Composer::default();
        c.text_input(&typed("a\nb"));
        assert_eq!(c.text(), "ab");
        assert!(c.take_submit());
    }

    /// Shift+Return breaks the line and a paste keeps its breaks; an input
    /// method's Enter still sends, and the prompt's own breaks stay.
    #[test]
    fn shift_return_and_a_paste_keep_their_line_breaks() {
        let mut c = Composer::default();
        c.text_input(&typed("first"));
        let shifted = KeyEvent { key_code: KeyCode::ReturnKey, modifiers: KeyModifiers { shift: true, ..Default::default() }, ..Default::default() };
        assert_eq!(key(&shifted), Key::NewLine);
        c.newline();
        // A platform that types that break as text too: dropped once.
        assert!(!c.text_input(&typed("\r")));
        c.text_input(&typed("second"));
        assert_eq!(c.text(), "first\nsecond");
        assert!(!c.take_submit());
        c.text_input(&TextInputEvent { input: "\r\nthird\r\nfourth".into(), was_paste: true, ..Default::default() });
        assert_eq!(c.text(), "first\nsecond\nthird\nfourth");
        assert!(!c.take_submit(), "a paste never sends");
        c.text_input(&typed("\n"));
        assert_eq!(c.text(), "first\nsecond\nthird\nfourth");
        assert!(c.take_submit());
    }

    fn moved(dir: Dir, select: bool, word: bool, line: bool) -> Motion {
        Motion { dir, select, word, line }
    }

    /// The caret moves and typing goes in at it, over a selection.
    #[test]
    fn typing_goes_in_at_the_caret_over_the_selection() {
        let mut c = Composer::default();
        c.text_input(&typed("hello world"));
        c.motion(moved(Dir::Left, false, true, false), &[]);
        assert_eq!(c.cursor(), 6, "Option+Left: the word's start");
        c.text_input(&typed("big "));
        assert_eq!(c.text(), "hello big world");
        // Shift+Option+Right selects "world"; typing replaces it.
        c.motion(moved(Dir::Right, true, true, false), &[]);
        assert_eq!(c.selected_text(), "world");
        c.text_input(&typed("there"));
        assert_eq!(c.text(), "hello big there");
        // Left without Shift collapses a selection to its start.
        c.select(6, 9);
        c.motion(moved(Dir::Left, false, false, false), &[]);
        assert_eq!((c.cursor(), c.selection().is_empty()), (6, true));
        // Backspace and Delete take the selection, else one character.
        c.select(0, 6);
        assert!(c.backspace());
        assert_eq!(c.text(), "big there");
        c.move_to(0, false);
        assert!(c.delete_forward());
        assert_eq!(c.text(), "ig there");
        assert!(!c.backspace(), "nothing before the caret");
    }

    #[test]
    fn home_end_and_command_go_to_the_lines_and_the_texts_ends() {
        let mut c = Composer::default();
        c.text_input(&typed("one"));
        c.newline();
        c.text_input(&typed("two"));
        c.motion(moved(Dir::Home, false, false, false), &[]);
        assert_eq!(c.cursor(), 4, "Home: this line's start");
        c.motion(moved(Dir::Right, false, false, true), &[]);
        assert_eq!(c.cursor(), 7, "Command+Right: this line's end");
        c.motion(moved(Dir::Up, true, false, true), &[]);
        assert_eq!(c.selection(), 0..7, "Shift+Command+Up selects to the text's start");
        c.select_all();
        assert_eq!(c.selected_text(), "one\ntwo");
    }

    /// Up and Down go by the drawn lines, keeping the caret's x.
    #[test]
    fn up_and_down_follow_the_drawn_lines() {
        let mut c = Composer::default();
        c.text_input(&typed("abcd efgh"));
        // Drawn as "abcd " and "efgh", 10 px a character.
        let xs = |n: usize| (0..=n).map(|i| i as f64 * 10.0).collect::<Vec<_>>();
        let layout = vec![LineLayout { start: 0, xs: xs(5) }, LineLayout { start: 5, xs: xs(4) }];
        c.move_to(7, false);
        c.motion(moved(Dir::Up, false, false, false), &layout);
        assert_eq!(c.cursor(), 2, "the same x on the line above");
        c.motion(moved(Dir::Down, true, false, false), &layout);
        assert_eq!((c.cursor(), c.selection()), (7, 2..7));
        c.motion(moved(Dir::Down, false, false, false), &layout);
        assert_eq!(c.cursor(), 9, "below the last line: the text's end");
        assert_eq!(Composer::index_at(&layout, 1, 23.0), 7, "a click at x 23 on the second line");
    }

    #[test]
    fn a_double_click_takes_a_word_and_copy_and_cut_take_the_selection() {
        let mut c = Composer::default();
        c.text_input(&typed("send mail.notify now"));
        assert_eq!(c.word_at(6), 5..9, "a word stops at punctuation");
        assert_eq!(c.word_at(9), 9..10, "punctuation alone");
        assert_eq!(c.copy(false), None, "nothing selected");
        c.select(5, 16);
        assert_eq!(c.copy(false).as_deref(), Some("mail.notify"));
        assert_eq!(c.text(), "send mail.notify now");
        assert_eq!(c.copy(true).as_deref(), Some("mail.notify"));
        assert_eq!((c.text(), c.cursor()), ("send  now", 5));
    }

    /// An input method composes at the caret, mid-text too.
    #[test]
    fn a_composition_goes_in_at_the_caret() {
        let mut c = Composer::default();
        c.text_input(&typed("ab"));
        c.move_to(1, false);
        c.text_input(&composing("ni"));
        assert_eq!(c.text(), "anib");
        c.text_input(&typed("你"));
        assert_eq!((c.text(), c.cursor()), ("a你b", 2));
        assert_eq!(c.state().selection, CharOffset(2)..CharOffset(2));
    }

    /// The soft keyboard's Enter (its Send action) sends, in either pane,
    /// even when no pane holds the key focus at that moment.
    #[test]
    fn the_soft_keyboards_enter_sends() {
        use makepad_widgets::makepad_platform::event::ImeAction;
        for action in [ImeAction::Send, ImeAction::Done, ImeAction::Go, ImeAction::Search, ImeAction::Unspecified, ImeAction::None] {
            assert!(ime_action_sends(action), "{action:?}");
        }
        assert!(!ime_action_sends(ImeAction::Next));
        assert!(!ime_action_sends(ImeAction::Previous));
        assert_eq!(ime_target(false, true), Some(Pane::System), "the system chat's prompt");
        assert_eq!(ime_target(true, false), Some(Pane::App), "the Ask panel's prompt");
    }

    /// With the system chat open but ANOTHER field focused (an app's text
    /// input, a host sheet), that field's Send and typing are its own: the
    /// chat's draft is not sent and nothing is swallowed.
    #[test]
    fn another_focused_field_keeps_its_keyboard() {
        assert_eq!(ime_target(false, false), None, "no chat prompt holds the focus");
        assert_eq!(text_target(false, false, true), None, "another field is focused");
        assert_eq!(text_target(false, true, true), None);
        assert_eq!(text_target(true, false, true), Some(Pane::System), "nothing focused: the open chat");
        assert_eq!(text_target(true, true, true), Some(Pane::App));
        assert_eq!(text_target(true, false, false), None);
    }

    /// The first character after the prompt takes the focus is kept (the
    /// ROM's Home once dropped it): the input method asks for the empty
    /// state, then its first edit lands whole, by full state or by text.
    #[test]
    fn the_first_character_after_focus_is_kept() {
        let mut c = Composer::default();
        let asked = c.state();
        assert_eq!((asked.text.as_str(), asked.selection.clone(), asked.composition.clone()), ("", CharOffset(0)..CharOffset(0), None));
        assert!(c.text_input(&ime_state("H", Some(0..1))));
        assert_eq!(c.text(), "H");
        c.text_input(&ime_state("Hi", Some(0..2)));
        assert_eq!(c.text(), "Hi");
        let mut c = Composer::default();
        assert!(c.text_input(&typed("H")));
        assert_eq!(c.text(), "H");
        // A composing first character, committed.
        let mut c = Composer::default();
        c.text_input(&composing("H"));
        c.text_input(&typed("H"));
        assert_eq!(c.text(), "H");
    }

    #[test]
    fn a_range_replacement_edits_in_place() {
        let mut c = Composer::default();
        c.text_input(&typed("teh cat"));
        c.text_input(&TextInputEvent { input: "the".into(), replace_range: Some((CharOffset(0), CharOffset(3))), ..Default::default() });
        assert_eq!(c.text(), "the cat");
    }
}
