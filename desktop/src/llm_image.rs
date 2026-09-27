//! The desktop's image source for AI providers' QR import (the `llm` host
//! service's `QrImagePicker`, OctoSense-System-Apps
//! `apps/ai-providers/host-service`): Makepad's native open panel filtered to
//! PNG and JPEG, and image files dropped on the AI providers window while
//! its import sheet is up. Either way the service gets the file's bytes and
//! reads the code out of them on a worker; the sheet then asks for the PIN.
//!
//! Test hook: `OCTOSENSE_LLM_TEST_IMAGE=<path>` makes "Choose image" answer
//! with that file at once, without a panel (the remote e2e run is hidden and
//! cannot click through a modal). Development only.
use makepad_widgets::makepad_platform::file_dialogs::{FileDialog, FileDialogAction};
use makepad_widgets::makepad_platform::thread::SignalToUI;
use makepad_widgets::*;
use octosense_llm_service::{ImageDone, PickError, QrImagePicker};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// The system app whose window takes dropped images (its short id).
const APP: &str = "ai-providers";
/// Larger files are not read at all; the service refuses over 20 MB anyway.
const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
const TEST_IMAGE_ENV: &str = "OCTOSENSE_LLM_TEST_IMAGE";

fn dialog_id() -> LiveId {
    live_id!(octosense_llm_qr_image)
}

#[derive(Default)]
pub struct DesktopPicker {
    /// The pick waiting on the open panel.
    pending: Mutex<Option<ImageDone>>,
    /// The panel should open on the next UI event.
    open: AtomicBool,
}

impl QrImagePicker for DesktopPicker {
    fn pick(&self, done: ImageDone) {
        if let Some(path) = std::env::var_os(TEST_IMAGE_ENV).filter(|p| !p.is_empty()) {
            let path = PathBuf::from(path);
            std::thread::spawn(move || done(read_image(&path)));
            return;
        }
        if let Some(earlier) = self.pending.lock().unwrap().replace(done) {
            earlier(Err(PickError::Cancelled));
        }
        self.open.store(true, Ordering::SeqCst);
        SignalToUI::set_ui_signal();
    }
}

/// The one picker the service is registered with.
pub fn picker() -> Arc<DesktopPicker> {
    static PICKER: OnceLock<Arc<DesktopPicker>> = OnceLock::new();
    PICKER.get_or_init(Arc::default).clone()
}

fn is_image(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
        Some("png" | "jpg" | "jpeg")
    )
}

/// A chosen or dropped file's bytes, off the UI thread.
fn read_image(path: &Path) -> Result<Vec<u8>, PickError> {
    let size = std::fs::metadata(path).map_err(|e| PickError::Failed(e.kind().to_string()))?.len();
    if size > MAX_FILE_BYTES {
        return Err(PickError::Failed("larger than 20 MB".into()));
    }
    std::fs::read(path).map_err(|e| PickError::Failed(e.kind().to_string()))
}

/// On every UI event: open the panel a pick asked for.
pub fn open_requested(cx: &mut Cx) {
    let picker = picker();
    if picker.open.swap(false, Ordering::SeqCst) {
        cx.open_select_file_dialog(
            FileDialog::new()
                .set_id(dialog_id())
                .set_title("Choose a picture of the provider code".into())
                .add_filter("Images".into(), vec!["png".into(), "jpg".into(), "jpeg".into()]),
        );
    }
}

/// The panel's answer, from the actions pass.
pub fn handle_actions(actions: &Actions) {
    for action in actions {
        let Some(answer) = action.downcast_ref::<FileDialogAction>() else {
            continue;
        };
        let result = match answer {
            FileDialogAction::FileSelected { id, paths } if *id == dialog_id() => paths.first().cloned().ok_or(PickError::Cancelled),
            FileDialogAction::FileCancelled { id } if *id == dialog_id() => Err(PickError::Cancelled),
            _ => continue,
        };
        let Some(done) = picker().pending.lock().unwrap().take() else {
            continue;
        };
        match result {
            Ok(path) if is_image(&path) => {
                std::thread::spawn(move || done(read_image(&path)));
            }
            Ok(_) => done(Err(PickError::Failed("not a PNG or JPEG file".into()))),
            Err(e) => done(Err(e)),
        }
    }
}

/// The one image file a drag or drop carries.
fn dropped_image(items: &[DragItem]) -> Option<PathBuf> {
    match items {
        [DragItem::FilePath { path, .. }] if is_image(Path::new(path)) => Some(PathBuf::from(path)),
        _ => None,
    }
}

/// A drag or drop of one image file over the AI providers window while its
/// import sheet waits for one: answered "copy", and on the drop the file goes
/// to the service. `app_at` names the app whose window is at a point. `true`
/// when the event was the service's.
pub fn handle_drop(event: &Event, app_at: impl Fn(Vec2d) -> Option<String>) -> bool {
    let over_import = |abs: Vec2d, items: &[DragItem]| {
        let path = dropped_image(items)?;
        (octosense_llm_service::wants_image() && app_at(abs).as_deref() == Some(APP)).then_some(path)
    };
    match event {
        Event::Drag(e) => {
            if over_import(e.abs, &e.items).is_some() {
                *e.response.lock().unwrap() = DragResponse::Copy;
                *e.handled.lock().unwrap() = true;
                return true;
            }
            false
        }
        Event::Drop(e) => {
            let Some(path) = over_import(e.abs, &e.items) else {
                return false;
            };
            *e.handled.lock().unwrap() = true;
            std::thread::spawn(move || {
                // A file that cannot be read is offered as nothing: the sheet
                // says it is not an image.
                let bytes = read_image(&path).unwrap_or_default();
                if !octosense_llm_service::offer_image(bytes) {
                    log!("llm: the import sheet closed before the dropped image was read");
                }
            });
            log!("llm: image dropped on the import sheet");
            true
        }
        _ => false,
    }
}
