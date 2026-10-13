//! The service's stable error codes (`apps/quickdeck/design/SERVICE.md`,
//! "Errors"): every `Err` of an open-document method, of `deck.cover` and of
//! the import guard starts with one, then a colon and plain words for a
//! person. The words never name a host path: `crate::serve` takes the
//! area's own spelling out of every error. The v1 file methods keep their
//! `deck.<method>: …` words, except a refusal of the import guard, which
//! carries its code on every path.

use std::fmt::Display;

use deckcraft_engine::EngineError;
use octosense_engine_area::door::{Refusal, Refused};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Code {
    /// No such file.
    NotFound,
    /// The engine couldn't read it; the engine's reason follows.
    Damaged,
    /// Not a presentation the engine reads: `.ppt`, `.key`, `.odp`, PDF.
    Unsupported,
    /// An encrypted `.pptx`, which needs a password.
    Protected,
    /// The deck's zip is beyond the import guard (a zip inside the zip, past
    /// a bound), or a command the door holds back until deckcraft bounds it
    /// (#448).
    Held,
    /// Over one of the service's caps.
    TooLarge,
    /// The storage has no room.
    StorageFull,
    /// The caller already has the most documents open.
    TooManyOpen,
    /// No such handle for this caller and scope.
    UnknownDoc,
    /// A slide or shape id the document doesn't have.
    UnknownId,
    /// The document has unsaved changes.
    Unsaved,
    /// Anything else the call got wrong: a path outside storage, a taken
    /// output name, nothing to undo or redo, a key the method doesn't take.
    Invalid,
}

impl Code {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Code::NotFound => "not_found",
            Code::Damaged => "damaged",
            Code::Unsupported => "unsupported",
            Code::Protected => "protected",
            Code::Held => "held",
            Code::TooLarge => "too_large",
            Code::StorageFull => "storage_full",
            Code::TooManyOpen => "too_many_open",
            Code::UnknownDoc => "unknown_doc",
            Code::UnknownId => "unknown_id",
            Code::Unsaved => "unsaved",
            Code::Invalid => "invalid",
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

/// `fail(TooLarge, …)`.
pub(crate) fn too_large(words: impl Display) -> String {
    fail(Code::TooLarge, words)
}

/// The door's refusal of a method's commands: a count or a limit is over a
/// cap (`too_large:`), a held command is `held:`, anything else `invalid:`
/// (a command the service built that the gate does not run is the
/// service's mistake, never run).
pub(crate) fn refusal(r: &Refusal) -> String {
    match r.kind {
        Refused::Count | Refused::Limit => too_large(&r.words),
        Refused::Held => fail(Code::Held, &r.words),
        Refused::Other => invalid(&r.words),
    }
}

/// The engine's refusal of a command the gate admitted: what the call asked
/// for does not fit the document.
pub(crate) fn engine(method: &str, id: &str, e: &EngineError) -> String {
    match e {
        EngineError::Internal(..) => fail(Code::Damaged, format!("{method}: the deck engine failed in `{id}`: {e}")),
        _ => invalid(format!("{method}: {e}")),
    }
}

/// A refusal from the area's write rules (`octosense_engine_area`): its
/// quota refusal (`octosense_engine_area::no_room`, the area's own test of
/// its own words) is `storage_full:`, and so is a disk that is full, which
/// the area passes on in the system's words (`ENOSPC`); anything else (a
/// name that is taken, a folder in the way) is `invalid:`.
pub(crate) fn area(words: impl Display) -> String {
    let words = words.to_string();
    if octosense_engine_area::no_room(&words) || words.contains("No space left") {
        fail(Code::StorageFull, words)
    } else {
        invalid(words)
    }
}

/// An error of the service's own file write: a full disk or quota
/// (`io::ErrorKind::StorageFull`, `QuotaExceeded`) is `storage_full:`,
/// anything else as [`area`].
pub(crate) fn io(words: impl Display, e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded => fail(Code::StorageFull, format!("{words}: {e}")),
        _ => area(format!("{words}: {e}")),
    }
}
