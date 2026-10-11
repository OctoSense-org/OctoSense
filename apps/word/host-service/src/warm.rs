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
//! The thread never goes through App Hub's service registry, so it holds no
//! lock a call takes on the way in; it reads font files only and uses no
//! network. A call that arrives while it runs shares only the font
//! database: before the catalog scan starts, the call scans and the thread
//! then finds it done; during the scan, the call waits for that one scan
//! (the database catalogs once per process) instead of starting another,
//! which never takes longer than scanning itself.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

static STARTED: AtomicBool = AtomicBool::new(false);

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
    let mut session = wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("Warm up"));
    let _ = session.run("file.info", &serde_json::json!({}));
}
