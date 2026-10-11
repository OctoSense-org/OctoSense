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
