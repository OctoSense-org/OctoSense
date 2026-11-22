//! The engine's first-call cost, paid once per process off the caller's
//! thread (in the shell, the UI thread: #399).
//!
//! wordcraft lays out pages with one process-wide font database
//! (`wordcraft_fonts::FontDb::global`). It is made on first use (the bundled
//! fonts parsed), catalogs the installed fonts the first time a family is
//! looked up by name (every font file in the system's and the user's font
//! folders opened for its name table), then loads the files of the family
//! asked for. Nothing of it is kept between processes, so the first call of
//! every process that lays out pages (`info`, or `convert` to PDF or PNG)
//! pays for all of it. [`warm`] pays it at registration instead, on a thread
//! of its own: it lays out a one-line document like the ones `new` writes,
//! which finds and loads the same fonts.
//!
//! Wordcraft scans the font catalog once, but loads a family's faces on demand.
//! If warm-up and a request load the same family together, one can see that the
//! other added the faces and select a fallback. Give the warm-up exclusive
//! access while it lays out its page. A request that arrives first can still
//! render cold. Requests wait on the engine worker, off the UI thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{RwLock, RwLockReadGuard};
use std::thread::JoinHandle;

static STARTED: AtomicBool = AtomicBool::new(false);
static FONT_ACCESS: RwLock<()> = RwLock::new(());

/// Keep the background warm-up out of the engine for the duration of a request.
pub(super) fn enter_request() -> RwLockReadGuard<'static, ()> {
    FONT_ACCESS.read().unwrap_or_else(|e| e.into_inner())
}

/// Start warming the engine on a background thread, once per process:
/// `Some(thread)` the first time, `None` after that. If the thread cannot
/// start, the first call pays, as without the warm-up.
pub fn warm() -> Option<JoinHandle<()>> {
    if STARTED.swap(true, Ordering::SeqCst) {
        return None;
    }
    std::thread::Builder::new().name("word-warm".into()).spawn(lay_out_one_line).ok()
}

/// A one-line document laid out, as `info` lays one out, and thrown away.
fn lay_out_one_line() {
    let _exclusive = FONT_ACCESS.write().unwrap_or_else(|e| e.into_inner());
    let mut session = wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("Warm up"));
    let _ = session.run("file.info", &serde_json::json!({}));
}
