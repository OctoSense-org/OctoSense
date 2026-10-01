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
//!
//! The caret is always at the end: the pane has no cursor of its own.

use makepad_widgets::makepad_platform::event::{CharOffset, FullTextState, TextInputEvent};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Composer {
    text: String,
    /// The characters (not bytes) being composed, if a word is.
    composition: Option<std::ops::Range<usize>>,
    /// A line break was typed: send.
    submit: bool,
    /// Shift+Return just broke the line: a platform that also types that
    /// break as text input has it dropped once.
    key_break: bool,
}

impl Composer {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Empty the prompt (after Send) and give back what it held.
    pub fn take(&mut self) -> String {
        self.composition = None;
        std::mem::take(&mut self.text)
    }

    pub fn clear(&mut self) {
        self.take();
    }

    /// Backspace: the last character (a composition ends with it).
    pub fn backspace(&mut self) -> bool {
        self.composition = None;
        self.text.pop().is_some()
    }

    fn chars(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, chars: usize) -> usize {
        CharOffset(chars).to_byte_index(&self.text)
    }

    /// Shift+Return: a line break at the end (the caret's place).
    pub fn newline(&mut self) {
        self.composition = None;
        self.text.push('\n');
        self.key_break = true;
    }

    /// One text-input event. True when the text changed.
    pub fn text_input(&mut self, event: &TextInputEvent) -> bool {
        let is_break = |c: char| c == '\n' || c == '\r';
        // The break Shift+Return already made, typed again as text.
        if std::mem::take(&mut self.key_break) && !event.was_paste && event.full_state_sync.is_none() && !event.input.is_empty() && event.input.chars().all(is_break) {
            return false;
        }
        let before = self.text.clone();
        self.apply(event);
        if event.was_paste {
            // A paste keeps its lines, as plain line breaks.
            if self.text.contains('\r') {
                self.text = self.text.replace("\r\n", "\n").replace('\r', "\n");
            }
        } else {
            // A typed line break (an input method's Enter) sends: the new
            // breaks go, the prompt's own stay.
            let breaks = |s: &str| s.chars().filter(|c| is_break(*c)).count();
            let mut typed = breaks(&self.text).saturating_sub(breaks(&before));
            if typed > 0 {
                while typed > 0 {
                    match self.text.rfind(is_break) {
                        Some(i) => {
                            self.text.remove(i);
                        }
                        None => break,
                    }
                    typed -= 1;
                }
                self.composition = None;
                self.submit = true;
            }
        }
        self.text != before
    }

    /// A line break was typed since the last call (the caller sends).
    pub fn take_submit(&mut self) -> bool {
        std::mem::take(&mut self.submit)
    }

    fn apply(&mut self, event: &TextInputEvent) {
        if let Some(state) = &event.full_state_sync {
            // The input method's whole editor state.
            self.text = state.text.clone();
            let end = self.chars();
            self.composition = state.composition.as_ref().map(|c| c.start.0.min(end)..c.end.0.min(end)).filter(|c| c.start < c.end);
            return;
        }
        if let Some((start, end)) = event.replace_range {
            // A replacement of a range (iOS autocorrect, a paste over it).
            let (a, b) = (start.0.min(end.0), start.0.max(end.0));
            let (a, b) = (self.byte(a), self.byte(b));
            self.text.replace_range(a..b, &event.input);
            self.composition = None;
            return;
        }
        let input = event.input.as_str();
        match self.composition.clone() {
            // A composition preview replaces the one before it; a commit
            // replaces it for good.
            Some(c) => {
                let (a, b) = (self.byte(c.start), self.byte(c.end));
                self.text.replace_range(a..b, input);
                let len = input.chars().count();
                self.composition = (event.replace_last && len > 0).then(|| c.start..c.start + len);
            }
            None => {
                let start = self.chars();
                self.text.push_str(input);
                let len = input.chars().count();
                self.composition = (event.replace_last && len > 0).then(|| start..start + len);
            }
        }
    }

    /// The editor state the input method asks for: the text, the caret at
    /// its end, and the composition.
    pub fn state(&self) -> FullTextState {
        let end = CharOffset(self.chars());
        FullTextState { text: self.text.clone(), selection: end..end, composition: self.composition.clone().map(|c| CharOffset(c.start)..CharOffset(c.end)) }
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
