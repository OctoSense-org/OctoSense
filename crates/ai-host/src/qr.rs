//! AI providers' QR import: where the `llm` service gets a provider QR from.
//!
//! - **Camera** (Android): Makepad's `cx.show_qr_scanner()`, whose answer is
//!   one `NativeQrScanned` or `NativeQrCancelled` action.
//! - **Android image picker**: the shell's Android extension
//!   (`QrImagePickActivity`) opened with `cx.android_integration("qr.image",
//!   {"id":n})`; it answers with a `qr.image.result` packet naming a private
//!   cache file, which the shell passes to [`crate::qr_image_result`].
//! - **File panel** (desktop): Makepad's open panel filtered to PNG and JPEG.
//! - **Drops** (desktop): one image file dropped on the AI providers window
//!   while its import sheet waits for one ([`crate::handle_drop`]).
//!
//! Anything else takes a pasted code. The service calls the scanner and the
//! picker from any thread; each request goes through a [`Bridge`] to the UI
//! thread, which [`crate::handle_event`] pumps.
//!
//! Test hook: `OCTOSENSE_LLM_TEST_IMAGE=<path>` makes a pick answer with that
//! file at once, without a panel (hidden-window e2e runs cannot click through
//! a modal). Development only.

use crate::bridge::Bridge;
use makepad_widgets::*;
use std::path::{Path, PathBuf};

#[cfg(feature = "llm")]
pub use octosense_llm_service::PickError;

/// Stand-in so the crate's surface is the same without the `llm` service.
#[cfg(not(feature = "llm"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickError {
    Cancelled,
    Failed(String),
}

/// Where a picture of a provider QR comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSource {
    /// No picture: the import sheet takes a pasted code.
    None,
    /// The shell's Android extension (`qr.image` / `qr.image.result`).
    AndroidExtension,
    /// Makepad's open panel (desktop).
    FilePanel,
}

/// How AI providers imports a provider QR on this shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QrImport {
    /// Scan with the camera (Android).
    pub camera: bool,
    /// Pick a picture of the code.
    pub image: ImageSource,
    /// Take an image file dropped on the AI providers window (desktop).
    pub drops: bool,
}

impl QrImport {
    /// What this platform offers: on Android the camera and the Home
    /// extension's image picker, on a desktop the open panel and drops,
    /// elsewhere (iOS, OpenHarmony) a pasted code.
    pub fn platform() -> Self {
        if cfg!(target_os = "android") {
            QrImport { camera: true, image: ImageSource::AndroidExtension, drops: false }
        } else if cfg!(any(target_os = "ios", target_env = "ohos")) {
            Self::paste_only()
        } else {
            QrImport { camera: false, image: ImageSource::FilePanel, drops: true }
        }
    }

    /// A pasted code only: no scanner, no picker, no drops.
    pub const fn paste_only() -> Self {
        QrImport { camera: false, image: ImageSource::None, drops: false }
    }
}

/// The system app whose window takes dropped images (its short id).
pub const DROP_APP: &str = "ai-providers";
/// Larger files are not read at all; the service refuses over 20 MB anyway.
const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
#[cfg(feature = "llm")]
const TEST_IMAGE_ENV: &str = "OCTOSENSE_LLM_TEST_IMAGE";

pub(crate) static SCAN: Bridge<Result<String, String>> = Bridge::new();
pub(crate) static PICK: Bridge<Result<Vec<u8>, PickError>> = Bridge::new();

#[cfg(feature = "llm")]
pub(crate) struct CameraScanner;

#[cfg(feature = "llm")]
impl octosense_llm_service::QrScanner for CameraScanner {
    fn scan(&self, done: octosense_llm_service::ScanDone) {
        SCAN.begin(done, Err("interrupted".into()));
    }
}

#[cfg(feature = "llm")]
pub(crate) struct ImagePicker;

#[cfg(feature = "llm")]
impl octosense_llm_service::QrImagePicker for ImagePicker {
    fn pick(&self, done: octosense_llm_service::ImageDone) {
        if let Some(path) = std::env::var_os(TEST_IMAGE_ENV).filter(|p| !p.is_empty()) {
            let path = PathBuf::from(path);
            std::thread::spawn(move || done(read_image(&path)));
            return;
        }
        PICK.begin(done, Err(PickError::Cancelled));
    }
}

fn dialog_id() -> LiveId {
    live_id!(octosense_llm_qr_image)
}

fn is_image(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
        Some("png" | "jpg" | "jpeg")
    )
}

/// A chosen or dropped file's bytes, off the UI thread.
pub(crate) fn read_image(path: &Path) -> Result<Vec<u8>, PickError> {
    let size = std::fs::metadata(path).map_err(|e| PickError::Failed(e.kind().to_string()))?.len();
    if size > MAX_FILE_BYTES {
        return Err(PickError::Failed("larger than 20 MB".into()));
    }
    std::fs::read(path).map_err(|e| PickError::Failed(e.kind().to_string()))
}

/// A `qr.image.result` packet's answer: the image's bytes (the file is read
/// and removed), or why there are none.
pub(crate) fn picked(status: &str, detail: &str) -> Result<Vec<u8>, PickError> {
    match status {
        "ok" => {
            let path = Path::new(detail);
            let bytes = std::fs::read(path).map_err(|e| PickError::Failed(format!("The image could not be read ({e}).")));
            let _ = std::fs::remove_file(path);
            bytes
        }
        "cancelled" => Err(PickError::Cancelled),
        _ => Err(PickError::Failed(match detail {
            "too_large" => "That image is too large.".into(),
            "picker_unavailable" => "No image picker is available.".into(),
            _ => "The image could not be read.".into(),
        })),
    }
}

/// UI thread, every event: open what the service asked for, and complete a
/// scan or a panel pick from the platform's answer.
pub(crate) fn pump(cx: &mut Cx, event: &Event, import: QrImport) {
    if SCAN.take_open().is_some() {
        log!("llm: opening the QR scanner");
        cx.show_qr_scanner();
    }
    if let Some(id) = PICK.take_open() {
        match import.image {
            ImageSource::AndroidExtension if cfg!(target_os = "android") => {
                log!("llm: opening the image picker");
                cx.android_integration("qr.image", &format!("{{\"id\":{id}}}"));
            }
            ImageSource::FilePanel if cfg!(not(any(target_os = "android", target_os = "ios"))) => {
                cx.open_select_file_dialog(
                    makepad_widgets::makepad_platform::file_dialogs::FileDialog::new()
                        .set_id(dialog_id())
                        .set_title("Choose a picture of the provider code".into())
                        .add_filter("Images".into(), vec!["png".into(), "jpg".into(), "jpeg".into()]),
                );
            }
            _ => {
                PICK.finish(Some(id), Err(PickError::Failed("This device has no image picker.".into())));
            }
        }
    }
    let Event::Actions(actions) = event else {
        return;
    };
    use makepad_widgets::makepad_platform::event::{NativeQrCancelled, NativeQrScanned};
    use makepad_widgets::makepad_platform::file_dialogs::FileDialogAction;
    for action in actions {
        if let Some(scan) = action.downcast_ref::<NativeQrScanned>() {
            if SCAN.finish(None, Ok(scan.json.clone())) {
                log!("llm: QR scanned ({} characters)", scan.json.len());
            }
        } else if let Some(cancel) = action.downcast_ref::<NativeQrCancelled>() {
            if SCAN.finish(None, Err(cancel.reason.clone())) {
                log!("llm: QR scan ended without a code: {}", cancel.reason);
            }
        } else if let Some(answer) = action.downcast_ref::<FileDialogAction>() {
            let result = match answer {
                FileDialogAction::FileSelected { id, paths } if *id == dialog_id() => {
                    paths.first().cloned().ok_or(PickError::Cancelled)
                }
                FileDialogAction::FileCancelled { id } if *id == dialog_id() => Err(PickError::Cancelled),
                _ => continue,
            };
            let Some(done) = PICK.take(None) else {
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
}

/// The Android extension's `qr.image.result`.
pub(crate) fn image_result(id: u64, status: &str, detail: &str) {
    let result = picked(status, detail);
    match &result {
        Ok(bytes) => log!("llm: image picked ({} bytes)", bytes.len()),
        Err(e) => log!("llm: image pick ended: {e:?}"),
    }
    if !PICK.finish(Some(id), result) {
        log!("llm: image pick {id} answered after it was superseded");
    }
}

/// The one image file a drag or drop carries.
#[cfg_attr(not(feature = "llm"), allow(dead_code))]
fn dropped_image(items: &[DragItem]) -> Option<PathBuf> {
    match items {
        [DragItem::FilePath { path, .. }] if is_image(Path::new(path)) => Some(PathBuf::from(path)),
        _ => None,
    }
}

/// A drag or drop of one image file over the AI providers window while its
/// import sheet waits for one: answered "copy", and on the drop the file goes
/// to the service.
#[cfg(feature = "llm")]
pub(crate) fn drop_event(event: &Event, app_at: &dyn Fn(Vec2d) -> Option<String>) -> bool {
    let over_import = |abs: Vec2d, items: &[DragItem]| {
        let path = dropped_image(items)?;
        (octosense_llm_service::wants_image() && app_at(abs).as_deref() == Some(DROP_APP)).then_some(path)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The picker's packet: a file is read and removed; cancel and failure
    /// map to the service's errors.
    #[test]
    fn a_picked_image_is_read_once_and_removed() {
        let dir = std::env::temp_dir().join(format!("octosense-llm-pick-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("qr-import-image");
        std::fs::write(&file, b"\x89PNG").unwrap();
        assert_eq!(picked("ok", file.to_str().unwrap()), Ok(b"\x89PNG".to_vec()));
        assert!(!file.exists());
        assert!(matches!(picked("ok", file.to_str().unwrap()), Err(PickError::Failed(_))));
        assert_eq!(picked("cancelled", ""), Err(PickError::Cancelled));
        assert_eq!(picked("error", "too_large"), Err(PickError::Failed("That image is too large.".into())));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A desktop pick or drop reads PNG and JPEG files only, and never a
    /// file over the service's limit.
    #[test]
    fn only_small_png_and_jpeg_files_are_read() {
        assert!(is_image(Path::new("/x/code.PNG")) && is_image(Path::new("a.jpeg")) && is_image(Path::new("a.jpg")));
        assert!(!is_image(Path::new("/x/code.gif")) && !is_image(Path::new("/x/png")));
        let one = |path: &str| vec![DragItem::FilePath { path: path.into(), internal_id: None }];
        assert_eq!(dropped_image(&one("/x/code.png")), Some(PathBuf::from("/x/code.png")));
        assert_eq!(dropped_image(&one("/x/notes.txt")), None);
        let mut two = one("/x/a.png");
        two.extend(one("/x/b.png"));
        assert_eq!(dropped_image(&two), None, "one image at a time");
        assert!(matches!(read_image(Path::new("/nonexistent/code.png")), Err(PickError::Failed(_))));
    }

    #[test]
    fn each_platform_imports_the_way_its_shell_can() {
        let import = QrImport::platform();
        if cfg!(target_os = "android") {
            assert!(import.camera);
            assert_eq!(import.image, ImageSource::AndroidExtension);
        } else if cfg!(any(target_os = "ios", target_env = "ohos")) {
            assert_eq!(import, QrImport::paste_only());
        } else {
            assert_eq!(import, QrImport { camera: false, image: ImageSource::FilePanel, drops: true });
        }
    }
}
