//! The host owns shutdown. The view can request it, never bypass app vetoes.
use octosense_updater::macos::Prepared;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
};

static PREPARED: Mutex<Option<Prepared>> = Mutex::new(None);
static REQUEST: AtomicBool = AtomicBool::new(false);
static NOTICE: OnceLock<(String, String)> = OnceLock::new();
static NOTICE_SHOWN: AtomicBool = AtomicBool::new(false);

pub(crate) fn request(prepared: Prepared) {
    *PREPARED.lock().unwrap_or_else(|e| e.into_inner()) = Some(prepared);
    REQUEST.store(true, Ordering::Release);
}
pub fn take_request() -> bool {
    REQUEST.swap(false, Ordering::AcqRel)
}
pub fn pending() -> bool {
    PREPARED.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}
pub fn cancel() {
    REQUEST.store(false, Ordering::Release);
    PREPARED.lock().unwrap_or_else(|e| e.into_inner()).take();
}
pub fn commit() -> Result<(), String> {
    PREPARED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .ok_or_else(|| "The update was cancelled before shutdown.".to_string())?
        .commit()
}
pub fn acknowledge_startup() {
    octosense_updater::macos::acknowledge_startup();
}

/// At startup, on the updater's sweep thread: an earlier installation's
/// outcome the person has not seen, then removing retained update copies
/// beside the installed app that nothing needs.
pub(crate) fn startup(cache: &Path) {
    if let Some(notice) = octosense_updater::macos::take_install_notice(cache) {
        let _ = NOTICE.set(notice);
        makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
    }
    octosense_updater::macos::sweep_backups();
}

/// A failed, cancelled or rolled-back update, as (title, message), shown once
/// so it reaches the person without a log. It includes the helper's fixed
/// rollback signal, set when it reopens the previous app.
pub fn take_install_notice() -> Option<(String, String)> {
    let notice = NOTICE.get()?;
    (!NOTICE_SHOWN.swap(true, Ordering::AcqRel)).then(|| notice.clone())
}
