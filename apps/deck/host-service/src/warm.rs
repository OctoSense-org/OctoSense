//! The engine's first-call cost, paid once per process off the caller's
//! thread (in the shell, the UI thread: #399).
//!
//! deckcraft lays out slide text with one process-wide font database
//! (`deckcraft_fonts::FontDb::global`). It is made on first use (the bundled
//! fonts parsed), catalogs the installed fonts the first time a family is
//! looked up by name (every font file in the system's and the user's font
//! folders opened for its name table), then loads the files of the family
//! asked for. Nothing of it is kept between processes, so the first `render`
//! or PDF `convert` of every process pays for all of it. [`warm`] pays it at
//! registration instead, on a thread of its own: it draws one tiny slide of
//! the deck `new` makes, which finds and loads the same fonts.
//!
//! The thread never goes through App Hub's service registry, so it holds no
//! lock a call takes on the way in; it reads font files only and uses no
//! network. A call that arrives while it runs waits for it to finish
//! ([`ready`]) before the engine does any work. The font database is not
//! safe to load one family from two threads at once: a lookup that finds
//! none of a family's faces yet loads its files, finds every face already
//! added by the other thread, and falls back to the last-resort face, so a
//! slide's title was drawn in Ubuntu Light once (the deck service's tests
//! caught it), a drawing the render cache and a cover would keep. The wait
//! is never longer than the warm-up, most of which (the catalog scan, the
//! default families' files) the call would otherwise pay itself, and
//! there is nothing to wait for once it is done. Without a warm-up (a
//! service registered alone, as in tests), the first call draws the slide
//! itself, on its own thread, and every other caller waits for that.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::thread::JoinHandle;

static STARTED: AtomicBool = AtomicBool::new(false);
/// Set once the warm-up's slide is drawn (or the engine failed to draw it).
static DONE: OnceLock<()> = OnceLock::new();

/// Start warming the engine on a background thread, once per process:
/// `Some(thread)` the first time, `None` after that. If the thread cannot
/// start, the first call pays, as without the warm-up.
pub fn warm() -> Option<JoinHandle<()>> {
    if STARTED.swap(true, Ordering::SeqCst) {
        return None;
    }
    std::thread::Builder::new().name("deck-warm".into()).spawn(ready).ok()
}

/// The warm-up, done: drawn here if no thread has started it, or waited
/// for while one draws it (module doc). Every call runs it first.
pub(crate) fn ready() {
    DONE.get_or_init(|| {
        // A failure to draw leaves the first real call to fail as it would.
        let _ = catch_unwind(AssertUnwindSafe(draw_one_slide));
    });
}

/// A slide of the deck `new` makes, drawn small and thrown away.
fn draw_one_slide() {
    let mut deck = deckcraft_model::defaults::blank_presentation(deckcraft_model::defaults::WIDE, Default::default(), false);
    deckcraft_format::outline_to_slides(&mut deck, "Warm\n\tup\n");
    let _ = deckcraft_engine::cmd::file::render_png(&deck, 0, 0.05, false);
}
