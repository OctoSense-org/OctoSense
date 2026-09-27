//! The `llm` host service on Home (ADR 0004): the AI providers system app
//! (`os.ai-providers`) edits the LLM providers of the shell's octos kernel
//! through `octosense_llm_service`, which Home registers at startup with the
//! things only a shell has:
//!
//! - the kernel's octos home (`core_dir`), where the service writes
//!   `profiles/_main.json`. The kernel is a shell service
//!   (`octosense_octos_core`, feature `octos-core`, on by default and in every
//!   native mobile build): Home configures it here with the app's data dir,
//!   AppCard and other consumers connect to it, and the service restarts it
//!   after every change (its `octos-core` feature), so the new provider set
//!   takes effect; connected consumers reconnect to the fresh kernel;
//! - on Android, a camera QR scanner (Makepad's `cx.show_qr_scanner()`, whose
//!   answer is one `NativeQrScanned` or `NativeQrCancelled` action) and an
//!   image picker (`QrImagePickActivity`: the system picker, its bytes in a
//!   private cache file named on the `qr.image.result` packet).
//!
//! The service calls the scanner and the picker from any thread.
//! Each request parks its completion in a [`Bridge`] and wakes the UI thread,
//! which opens the platform surface on its next event and completes the
//! request from the platform's answer. One request of a kind is outstanding
//! at a time: a newer one answers the older as interrupted.

use crate::*;
use makepad_widgets::makepad_platform::thread::SignalToUI;
use octosense_llm_service::PickError;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

type Done<R> = Box<dyn FnOnce(R) + Send>;

/// One outstanding request whose answer comes from the UI thread: the
/// completion, an id to match a late answer against, and whether the UI
/// thread still has to open the surface for it.
pub struct Bridge<R> {
    pending: Mutex<Option<(u64, Done<R>)>>,
    open: AtomicBool,
    next: AtomicU64,
}

impl<R> Bridge<R> {
    pub const fn new() -> Self {
        Bridge { pending: Mutex::new(None), open: AtomicBool::new(false), next: AtomicU64::new(1) }
    }

    /// Park `done` (any thread) and ask the UI thread to open the surface.
    /// An earlier request still waiting is answered with `superseded`.
    pub fn begin(&self, done: Done<R>, superseded: R) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let earlier = self.pending.lock().unwrap().replace((id, done));
        self.open.store(true, Ordering::Release);
        if let Some((_, earlier)) = earlier {
            earlier(superseded);
        }
        SignalToUI::set_ui_signal();
        id
    }

    /// UI thread: the request to open a surface for, once.
    pub fn take_open(&self) -> Option<u64> {
        if !self.open.swap(false, Ordering::AcqRel) {
            return None;
        }
        self.pending.lock().unwrap().as_ref().map(|(id, _)| *id)
    }

    /// Whether a request waits for its answer.
    pub fn is_pending(&self) -> bool {
        self.pending.lock().unwrap().is_some()
    }

    /// Answer the outstanding request (`id`: only that one). False when
    /// nothing waited, e.g. a scan another part of the app started.
    pub fn finish(&self, id: Option<u64>, result: R) -> bool {
        let done = {
            let mut pending = self.pending.lock().unwrap();
            match (pending.as_ref(), id) {
                (Some((current, _)), Some(id)) if *current != id => None,
                _ => pending.take(),
            }
        };
        match done {
            Some((_, done)) => {
                done(result);
                true
            }
            None => false,
        }
    }
}

static SCAN: Bridge<Result<String, String>> = Bridge::new();
static PICK: Bridge<Result<Vec<u8>, PickError>> = Bridge::new();

/// The camera scanner the service asks for a provider QR.
struct CameraScanner;

impl octosense_llm_service::QrScanner for CameraScanner {
    fn scan(&self, done: octosense_llm_service::ScanDone) {
        SCAN.begin(done, Err("interrupted".into()));
    }
}

/// The image picker the service asks for a picture of a provider QR.
struct ImagePicker;

impl octosense_llm_service::QrImagePicker for ImagePicker {
    fn pick(&self, done: octosense_llm_service::ImageDone) {
        PICK.begin(done, Err(PickError::Cancelled));
    }
}

/// Configure the shell's octos kernel (`octosense_octos_core`), once, before
/// anything connects to it: on a phone its core dir is
/// `<data dir>/octos-home/.octos`. Nothing starts until a consumer connects.
#[cfg(any(feature = "octos-core", native_mobile))]
pub fn configure_kernel(data_dir: Option<String>) {
    // The kernel's stderr and the core's starts and stops go to Home's log
    // (logcat on Android), not the `log` facade no logger listens to here.
    let mut options = octosense_octos_core::Options::default().log(|line| log!("{line}"));
    if let Some(dir) = data_dir.filter(|d| !d.is_empty()) {
        options = options.app_data_dir(dir);
    }
    octosense_octos_core::configure(options);
    match octosense_octos_core::launch() {
        Ok(_) => log!("octos: kernel service ready (starts on first use), core dir {:?}", octosense_octos_core::core_dir()),
        Err(why) => log!("octos: {why}; the providers are still saved under {:?}", octosense_octos_core::core_dir()),
    }
}

/// The kernel's octos home, where the `llm` service writes: the shell's
/// octos kernel's core dir (`OCTOS_APP_CORE_DIR`, else on a phone
/// `<data dir>/octos-home/.octos`, else `$HOME/octos-home/.octos`).
#[cfg(any(feature = "octos-core", native_mobile))]
pub fn core_dir(_data_dir: Option<String>) -> Option<PathBuf> {
    octosense_octos_core::core_dir()
}

/// Without the kernel service (a desktop build without `octos-core`): the
/// same rule, for the profile alone.
#[cfg(not(any(feature = "octos-core", native_mobile)))]
pub fn core_dir(data_dir: Option<String>) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("OCTOS_APP_CORE_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    if cfg!(any(target_os = "android", target_env = "ohos")) {
        if let Some(dir) = data_dir.filter(|d| !d.is_empty()) {
            return Some(PathBuf::from(dir).join("octos-home").join(".octos"));
        }
    }
    octosense_llm_config::profile::default_core_dir()
}

/// Register the `llm` service, once, before the first system app opens.
/// The scanner and the picker exist on Android only; elsewhere the import
/// sheet takes a pasted code.
pub fn register(core_dir: Option<PathBuf>) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let mut options = octosense_llm_service::Options::default();
        match &core_dir {
            Some(dir) => {
                log!("llm: provider profile under {}", dir.display());
                options = options.core_dir(dir.clone());
            }
            None => log!("llm: no core dir; the service uses its own default"),
        }
        #[cfg(target_os = "android")]
        {
            options = options
                .scanner(std::sync::Arc::new(CameraScanner))
                .image_picker(std::sync::Arc::new(ImagePicker));
        }
        // With `octos-core` the service itself restarts the kernel after a
        // change: no hook needed here.
        octosense_llm_service::register_with(options);
    });
}

/// A `qr.image.result` packet's answer: the image's bytes (the file is read
/// and removed), or why there are none.
fn picked(status: &str, detail: &str) -> Result<Vec<u8>, PickError> {
    match status {
        "ok" => {
            let path = std::path::Path::new(detail);
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

impl App {
    /// Every event, on the UI thread: open what the service asked for, and
    /// complete a scan from the scanner's answer.
    pub(crate) fn llm_host_event(&mut self, cx: &mut Cx, event: &Event) {
        if SCAN.take_open().is_some() {
            log!("llm: opening the QR scanner");
            cx.show_qr_scanner();
        }
        if let Some(id) = PICK.take_open() {
            log!("llm: opening the image picker");
            #[cfg(target_os = "android")]
            cx.android_integration("qr.image", &format!("{{\"id\":{id}}}"));
            #[cfg(not(target_os = "android"))]
            PICK.finish(Some(id), Err(PickError::Failed("This device has no image picker.".into())));
        }
        if let Event::Actions(actions) = event {
            for action in actions {
                if let Some(scan) = action.downcast_ref::<makepad_widgets::makepad_platform::event::NativeQrScanned>() {
                    if SCAN.finish(None, Ok(scan.json.clone())) {
                        log!("llm: QR scanned ({} characters)", scan.json.len());
                    }
                } else if let Some(cancel) = action.downcast_ref::<makepad_widgets::makepad_platform::event::NativeQrCancelled>() {
                    if SCAN.finish(None, Err(cancel.reason.clone())) {
                        log!("llm: QR scan ended without a code: {}", cancel.reason);
                    }
                }
            }
        }
    }

    /// `qr.image.result` from the Android extension.
    pub(crate) fn llm_image_packet(&mut self, id: u64, status: &str, detail: &str) {
        let result = picked(status, detail);
        match &result {
            Ok(bytes) => log!("llm: image picked ({} bytes)", bytes.len()),
            Err(e) => log!("llm: image pick ended: {e:?}"),
        }
        if !PICK.finish(Some(id), result) {
            log!("llm: image pick {id} answered after it was superseded");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn recorder<R: Send + 'static>() -> (Arc<Mutex<Vec<R>>>, impl Fn() -> Done<R>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        (seen, move || {
            let sink = sink.clone();
            Box::new(move |r| sink.lock().unwrap().push(r)) as Done<R>
        })
    }

    /// A scan opens the scanner once and completes from the answer, once;
    /// an answer nobody asked for (AppCard's own composer scan) is left alone.
    #[test]
    fn a_scan_opens_once_and_completes_once() {
        let bridge: Bridge<Result<String, String>> = Bridge::new();
        let (seen, done) = recorder();
        assert!(!bridge.finish(None, Ok("stray".into())));
        let id = bridge.begin(done(), Err("interrupted".into()));
        assert_eq!(bridge.take_open(), Some(id));
        assert_eq!(bridge.take_open(), None);
        assert!(bridge.is_pending());
        assert!(bridge.finish(None, Ok("OCTOS1E:abc".into())));
        assert!(!bridge.finish(None, Err("cancelled".into())));
        assert_eq!(*seen.lock().unwrap(), vec![Ok("OCTOS1E:abc".to_string())]);
    }

    /// One outstanding request: a newer one answers the older as superseded,
    /// and a late answer for the older id does not complete the newer.
    #[test]
    fn a_newer_request_supersedes_the_older() {
        let bridge: Bridge<Result<String, String>> = Bridge::new();
        let (seen, done) = recorder();
        let first = bridge.begin(done(), Err("interrupted".into()));
        let second = bridge.begin(done(), Err("interrupted".into()));
        assert_ne!(first, second);
        assert_eq!(*seen.lock().unwrap(), vec![Err("interrupted".to_string())]);
        assert!(!bridge.finish(Some(first), Ok("late".into())));
        assert!(bridge.finish(Some(second), Err("cancelled".into())));
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert_eq!(seen.lock().unwrap()[1], Err("cancelled".to_string()));
    }

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

    #[test]
    fn a_phone_core_dir_is_the_kernels_octos_home() {
        if std::env::var_os("OCTOS_APP_CORE_DIR").is_some() {
            return;
        }
        let dir = core_dir(Some("/data/user/0/app/files".into()));
        if cfg!(any(target_os = "android", target_env = "ohos")) {
            assert_eq!(dir, Some(PathBuf::from("/data/user/0/app/files/octos-home/.octos")));
        } else {
            assert_eq!(dir, octosense_llm_config::profile::default_core_dir());
        }
    }
}
