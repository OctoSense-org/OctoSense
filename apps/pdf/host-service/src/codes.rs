//! The service's stable error codes (`apps/pdftools/design/SERVICE.md`):
//! every `Err` starts with one, then a colon and plain words for a person.
//! The words never name a host path: [`crate::serve`] takes the area's own
//! spelling out of every error.

use std::fmt::Display;

use pdfcraft_automation::ToolError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Code {
    /// The file a call names is not there.
    NotFound,
    /// The engine cannot read the file; its reason follows.
    Damaged,
    /// The PDF needs a password, and the service does not open those.
    Protected,
    /// The app's storage has no room for the result.
    StorageFull,
    /// The caller already has the most documents open.
    TooManyOpen,
    /// The handle names no document this caller has open.
    UnknownDoc,
    /// The document has edits that were never saved.
    Unsaved,
    /// The request is malformed, names something the method does not take,
    /// or asks for something the document refuses.
    Invalid,
    /// The request is over one of the service's caps.
    TooLarge,
}

impl Code {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Code::NotFound => "not_found",
            Code::Damaged => "damaged",
            Code::Protected => "protected",
            Code::StorageFull => "storage_full",
            Code::TooManyOpen => "too_many_open",
            Code::UnknownDoc => "unknown_doc",
            Code::Unsaved => "unsaved",
            Code::Invalid => "invalid",
            Code::TooLarge => "too_large",
        }
    }
}

/// An error: `code`, a colon, and `words`.
pub(crate) fn fail(code: Code, words: impl Display) -> String {
    format!("{}: {words}", code.as_str())
}

/// `fail(Invalid, …)`, the most common refusal.
pub(crate) fn invalid(words: impl Display) -> String {
    fail(Code::Invalid, words)
}

/// Whether the engine's words say a password is needed (pdfcraft-render's
/// `NeedsPassword` and `WrongPassword` when it opens a document, and the
/// "it is password-protected" of a file it combines or inserts).
fn needs_password(words: &str) -> bool {
    words.contains("protected by a password") || words.contains("password is incorrect") || words.contains("password-protected")
}

/// The engine's refusal of a file it was asked to read (`doc_open`, and
/// the files an edit reads): a password is `protected:`, a missing file
/// `not_found:`, anything else `damaged:` with the engine's reason.
pub(crate) fn unreadable(e: &ToolError) -> String {
    let words = e.to_string();
    if needs_password(&words) {
        fail(Code::Protected, "this PDF is protected with a password, and PDF Tools does not open those yet")
    } else if words.contains("No such file") || words.contains("cannot find the file") {
        fail(Code::NotFound, words)
    } else {
        fail(Code::Damaged, words)
    }
}

/// The engine's refusal of an edit or a read of an open document: what
/// the call asked for does not fit the document (a page out of range, a
/// comment that is not there, nothing to undo, a read-only document).
/// A file the edit read that needs a password is `protected:`.
pub(crate) fn refused(e: &ToolError) -> String {
    let words = e.to_string();
    if needs_password(&words) {
        fail(Code::Protected, "that PDF is protected with a password, and PDF Tools does not open those yet")
    } else {
        invalid(words)
    }
}

/// The engine's refusal of a command that reads another file (pages
/// inserted from a PDF, files combined): that file needing a password is
/// `protected:`, missing `not_found:`, unreadable `damaged:`; anything
/// else is the request not fitting (`invalid:`).
pub(crate) fn read_or_refused(e: &ToolError) -> String {
    let words = e.to_string();
    if needs_password(&words) || words.contains("No such file") || words.contains("not a readable PDF") {
        unreadable(e)
    } else {
        invalid(words)
    }
}

/// A refusal from the area's write rules (`octosense_engine_area`): over
/// the quota is `storage_full:`, anything else (a name that is taken, a
/// folder in the way) `invalid:`.
pub(crate) fn area(words: impl Display) -> String {
    let words = words.to_string();
    if words.contains("bytes left in this storage") || words.contains("No space left") {
        fail(Code::StorageFull, words)
    } else {
        invalid(words)
    }
}

/// The engine's refusal of a text edit (`pdf.edit_text`, and `pdf.edit_check`
/// asking the same), in words for a person, still `invalid:` as SERVICE.md
/// has it: the engine's own words name a build setting, a font file or a
/// Unicode code point, never what to do.
///
/// - Text the paragraph's font can't show, which the engine then draws in a
///   stand-in font: Helvetica, Times or Courier show only Latin text, and its
///   fallback for everything else is a Japanese face this build leaves out
///   ("needs PdfCraft's Japanese fallback font, which this build doesn't
///   include"), lacks a character ("has no glyph for U+…"), or holds at most
///   240 different characters.
/// - A document the engine opened for reading only, or whose security
///   settings forbid changes (PDF Tools takes no passwords).
/// - A paragraph or line number that is no longer on the page.
///
/// Anything else keeps the engine's words.
pub(crate) fn edit_refused(words: &str, text: &str) -> String {
    let has = |s: &str| words.contains(s);
    if has("Japanese fallback font has no glyph for U+") || has("Japanese fallback glyph U+") {
        // One character the fallback face lacks (a build with the face).
        let missing = words.split("U+").nth(1).and_then(|hex| u32::from_str_radix(hex.get(..hex.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(hex.len()))?, 16).ok());
        if let Some(c) = missing.and_then(char::from_u32) {
            return invalid(format!("PDF Tools has no font that can draw {} yet, so it can't type it into this PDF.", quoted(&[c])));
        }
        return invalid(unshowable(text));
    }
    if has("Japanese replacement has too many unique characters") {
        return invalid("That's more different characters than PDF Tools can type into one paragraph: shorten the text, or edit it in parts.");
    }
    if has("Japanese fallback") || has("can show") || has("can't be shown") {
        return invalid(unshowable(text));
    }
    if has("can't be edited") {
        return invalid("This PDF opens for reading only, so PDF Tools can't change its text.");
    }
    if has("security settings don't allow") {
        return invalid("This PDF's security settings don't allow changes, so PDF Tools can't edit it.");
    }
    if has("is out of range") || has("has no paragraph") || has("has no line") || has("the page's content changed") {
        return invalid("That text isn't on the page any more: click it again.");
    }
    if has("has no font") {
        return invalid("PDF Tools can't edit this text: the PDF doesn't say which font draws it.");
    }
    if needs_password(words) {
        return fail(Code::Protected, "that PDF is protected with a password, and PDF Tools does not open those yet");
    }
    invalid(words)
}

/// Why `text` can't be typed into the PDF in a font PDF Tools has: Chinese,
/// Japanese or Korean text, or the characters no stand-in font shows.
fn unshowable(text: &str) -> String {
    if text.chars().any(is_cjk) {
        return "PDF Tools can't edit Chinese, Japanese or Korean text yet.".into();
    }
    let mut odd: Vec<char> = Vec::new();
    for c in text.chars().filter(|c| !c.is_whitespace() && !is_win_ansi(*c)) {
        if !odd.contains(&c) {
            odd.push(c);
        }
    }
    if odd.is_empty() {
        return "PDF Tools can't type this text into this PDF yet.".into();
    }
    format!("PDF Tools can't type {} into a PDF yet.", quoted(&odd))
}

/// Characters quoted for a sentence: “a”, “b” or “c”; more than five as the
/// first five “and others like them”.
fn quoted(chars: &[char]) -> String {
    let each: Vec<String> = chars.iter().take(5).map(|c| format!("“{c}”")).collect();
    if chars.len() > 5 {
        return format!("{} and others like them", each.join(", "));
    }
    match each.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
    }
}

/// Chinese, Japanese or Korean script, or its punctuation and full-width
/// forms.
pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11ff        // Hangul jamo
        | 0x2e80..=0x2fdf      // radicals
        | 0x3000..=0x303f      // CJK symbols and punctuation
        | 0x3040..=0x30ff      // Hiragana, Katakana
        | 0x3100..=0x318f      // Bopomofo, Hangul compatibility jamo
        | 0x31a0..=0x31ff      // Bopomofo extended, strokes, Katakana extensions
        | 0x3400..=0x4dbf      // CJK extension A
        | 0x4e00..=0x9fff      // CJK unified ideographs
        | 0xa960..=0xa97f      // Hangul jamo extended A
        | 0xac00..=0xd7ff      // Hangul syllables, jamo extended B
        | 0xf900..=0xfaff      // compatibility ideographs
        | 0xfe30..=0xfe4f      // compatibility forms
        | 0xff00..=0xffef      // half-width and full-width forms
        | 0x20000..=0x3134f)   // extensions B to G
}

/// The characters the engine's stand-in fonts show (WinAnsiEncoding), as
/// pdfcraft-edit's `is_win_ansi_char` lists them.
fn is_win_ansi(c: char) -> bool {
    matches!(c, '\u{20}'..='\u{7e}' | '\u{a0}'..='\u{ff}' | '€' | '‚' | '„' | '…' | '‘' | '’' | '“' | '”' | '•' | '–' | '—' | '™' | '\t')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The engine's own refusals of an edit, as pdfcraft-edit and
    /// pdfcraft-engine word them at the pinned revision, each in words
    /// for a person, still `invalid:`.
    #[test]
    fn an_edits_refusal_is_in_plain_words() {
        let no_face = "this text needs PdfCraft's Japanese fallback font, which this build doesn't include \
                       (official releases do; to build it in, set CRAFT_FONTS_DIR to a craft-fonts checkout)";
        let cases = [
            (no_face, "测试中文", "invalid: PDF Tools can't edit Chinese, Japanese or Korean text yet."),
            (no_face, "こんにちは", "invalid: PDF Tools can't edit Chinese, Japanese or Korean text yet."),
            (no_face, "안녕하세요", "invalid: PDF Tools can't edit Chinese, Japanese or Korean text yet."),
            (no_face, "Привет", "invalid: PDF Tools can't type “П”, “р”, “и”, “в”, “е” and others like them into a PDF yet."),
            (no_face, "x → y", "invalid: PDF Tools can't type “→” into a PDF yet."),
            (no_face, "x → y ↔ z", "invalid: PDF Tools can't type “→” or “↔” into a PDF yet."),
            (no_face, "Ωmega ≈ ∞", "invalid: PDF Tools can't type “Ω”, “≈” or “∞” into a PDF yet."),
            ("Japanese fallback font has no glyph for U+6D4B", "测试", "invalid: PDF Tools has no font that can draw “测” yet, so it can't type it into this PDF."),
            ("Japanese replacement has too many unique characters", "…", "invalid: That's more different characters than PDF Tools can type into one paragraph: shorten the text, or edit it in parts."),
            ("\"x ☃\" has characters neither ABCDEF+Arial nor Helvetica can show", "x ☃", "invalid: PDF Tools can't type “☃” into a PDF yet."),
            ("this document can't be edited: the document structure could not be read for editing", "x", "invalid: This PDF opens for reading only, so PDF Tools can't change its text."),
            (
                "the document's security settings don't allow changes to the document; open it with the owner password to make this change",
                "x",
                "invalid: This PDF's security settings don't allow changes, so PDF Tools can't edit it.",
            ),
            ("paragraph 9 is out of range: page 1 has 3 paragraphs", "x", "invalid: That text isn't on the page any more: click it again."),
            ("page 1 has no line 4", "x", "invalid: That text isn't on the page any more: click it again."),
            ("the paragraph has no font", "x", "invalid: PDF Tools can't edit this text: the PDF doesn't say which font draws it."),
            ("Edit text failed unexpectedly (boom); the document was not changed", "x", "invalid: Edit text failed unexpectedly (boom); the document was not changed"),
        ];
        for (engine, text, plain) in cases {
            assert_eq!(edit_refused(engine, text), plain, "{engine}");
        }
    }

    #[test]
    fn cjk_is_told_from_other_scripts() {
        for c in ['中', '文', '。', '「', 'あ', 'カ', '한', 'ㄅ', '！', '𠀀'] {
            assert!(is_cjk(c), "{c}");
        }
        for c in ['a', 'é', 'Ж', 'α', '→', 'ا', '€', ' '] {
            assert!(!is_cjk(c), "{c}");
        }
    }
}
