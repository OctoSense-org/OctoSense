//! Foreground, single-file transfer between native dialogs and the live app's
//! existing Splash jail. Neither a selected host path nor a provider URI crosses
//! the script boundary. See README.md for the public contract and integration.
use makepad_widgets::{
    makepad_platform::{
        file_dialogs::{self, FileDialog, FileDialogAccessGuard, FileDialogAction},
        thread::Lane,
        SignalToUI,
    },
    splash_storage::{self, PreparedStorageImport, StorageAccess, MAX_FILE_BYTES, MAX_IMPORT_BYTES},
    Cx, Event, LiveId,
};
use octosense_appstore::services::{
    self, AgentAccess, HostApiMethod, HostService, Replier, ServiceCall, ServiceHost,
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, TryLockError,
    },
    time::Duration,
};

#[cfg(test)]
mod tests;

const TIMEOUT: Duration = Duration::from_secs(300);
const MAX_SHARE_TEXT: usize = 8192;
/// The largest document `files.import` copies in. Android's document loader
/// holds the selection in the Java heap and then copies it into native
/// memory, so phones keep a smaller bound until it streams to a file.
const MAX_IMPORT: u64 = if cfg!(target_os = "android") { 16 * 1024 * 1024 } else { MAX_IMPORT_BYTES };
static BUSY: AtomicBool = AtomicBool::new(false);
static QUEUED: Mutex<Option<Work>> = Mutex::new(None);
static STAGED: Mutex<Option<Staged>> = Mutex::new(None);
thread_local! { static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) }; }

struct Foreground {
    app: Option<String>,
    background: bool,
    window_unfocused: bool,
}
impl Default for Foreground {
    fn default() -> Self {
        Self {
            app: None,
            background: false,
            // A hidden/inactive desktop window has no authority to open UI.
            window_unfocused: cfg!(target_os = "macos"),
        }
    }
}
impl Foreground {
    fn event(&mut self, event: &Event) {
        match event {
            Event::Pause | Event::Background => self.background = true,
            Event::Resume | Event::Foreground => self.background = false,
            Event::WindowLostFocus(_) => self.window_unfocused = true,
            Event::WindowGotFocus(_) => self.window_unfocused = false,
            _ => {}
        }
    }
    fn allows_launch(&self, app: &str, origin_may_prompt: bool, live_surface: bool) -> bool {
        origin_may_prompt && live_surface && !self.background && !self.window_unfocused
            && self.app.as_deref() == Some(app)
    }
}
thread_local! { static FOREGROUND: RefCell<Foreground> = RefCell::new(Foreground::default()); }

/// Host-owned current client/expanded Glance identity, never a script argument.
pub fn set_foreground_app(app: Option<String>) {
    FOREGROUND.with(|foreground| foreground.borrow_mut().app = app);
}


/// One dialog, including the worker holding its export snapshot. A slow
/// document provider cannot grow a queue of payloads when a request times out.
struct Reservation;
impl Reservation {
    fn acquire() -> Result<Self, String> {
        BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| "busy: Another file transfer is pending".into())
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}

#[derive(Debug, PartialEq)]
enum Operation {
    Import { path: String },
    PickPhoto { path: String },
    Share { text: String },
    Export { path: String, name: String },
}

impl Operation {
    fn needs_storage(&self) -> bool {
        !matches!(self, Self::Share { .. })
    }
    /// What the native loader may read for this selection: the operation's
    /// own bound, never more than the app's whole storage quota.
    fn selection_limit(&self, quota: u64) -> u64 {
        let bound = if matches!(self, Self::Import { .. }) { MAX_IMPORT } else { MAX_FILE_BYTES };
        bound.min(quota)
    }
}
/// Inspect bytes, never a filename or a provider-supplied MIME hint. This is a
/// format signature check, not a decoder or a claim that pixels are well formed.
fn image_mime(bytes: &[u8]) -> Result<&'static str, String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok("image/png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Ok("image/jpeg");
    }
    if bytes.len() >= 16
        && &bytes[..4] == b"RIFF"
        && &bytes[8..12] == b"WEBP"
        && matches!(&bytes[12..16], b"VP8 " | b"VP8L" | b"VP8X")
    {
        return Ok("image/webp");
    }
    Err("invalid_image: Choose a PNG, JPEG or WebP file; filename and MIME labels are not sufficient".into())
}
fn share_outcome(value: &Value) -> Result<Value, String> {
    match value["outcome"].as_str() {
        Some("opened") => Ok(json!({"handoff":"chooser_opened","delivery":"unknown"})),
        Some("error") => Err(match value["reason"].as_str() {
            Some("foreground_required") => "foreground_required: Return to the app before sharing",
            Some("invalid_arguments") => "invalid_arguments: Invalid native share request",
            _ => "share_unavailable: Android could not open a text share chooser",
        }
        .into()),
        _ => Err("invalid_response: Android did not confirm opening the share chooser".into()),
    }
}

fn parse_operation(method: &str, args: &Value) -> Result<Operation, String> {
    let args = args
        .as_object()
        .ok_or("invalid_arguments: Expected an object")?;
    if method == "share" {
        if args.len() != 1 {
            return Err("invalid_arguments: share accepts only text".into());
        }
        let text = args.get("text").and_then(Value::as_str)
            .filter(|text| !text.is_empty() && text.len() <= MAX_SHARE_TEXT
                && !text.chars().any(|c| c.is_control() && c != '\n' && c != '\t'))
            .ok_or("invalid_arguments: share text must be 1–8192 UTF-8 bytes without control characters")?;
        return Ok(Operation::Share { text: text.into() });
    }
    let export = match method {
        "import" | "pick_photo" => false,
        "export" => true,
        _ => return Err("method_unavailable: Unknown files method".into()),
    };
    if args
        .keys()
        .any(|key| key != "path" && !(export && key == "name"))
    {
        return Err("invalid_arguments: Only path and an optional export name are accepted".into());
    }
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty() && p.len() <= 2048)
        .ok_or("invalid_arguments: path must name an app file")?
        .to_owned();
    // This is only lexical validation; the live isolate supplies the actual
    // root, and StorageAccess checks it and its components before every IO.
    let root = std::path::Path::new("/");
    if splash_storage::resolve_jailed(root, &path)? == root {
        return Err("invalid_arguments: path must name a file".into());
    }
    if !export {
        return Ok(if method == "pick_photo" {
            Operation::PickPhoto { path }
        } else {
            Operation::Import { path }
        });
    }
    let name = match args.get("name") {
        Some(value) => value
            .as_str()
            .ok_or("invalid_arguments: name must be a basename")?,
        None => path.rsplit(['/', '\\']).next().unwrap_or(""),
    };
    if name.is_empty()
        || name.len() > 128
        || name.contains(['/', '\\'])
        || name == "."
        || name == ".."
    {
        return Err("invalid_arguments: name must be a basename".into());
    }
    splash_storage::resolve_jailed(root, name)?;
    let name = name.to_owned();
    Ok(Operation::Export { path, name })
}

fn admission(app: &str, storage: bool) -> Result<Value, String> {
    let loaded = crate::host_tools::script_apps::guidance(app)
        .map_err(|_| "permission_denied: App is no longer admitted")?;
    if !loaded.families.contains("files") || (storage && !loaded.families.contains("storage")) {
        return Err("permission_denied: File transfer requires files and storage grants".into());
    }
    Ok(loaded.manifest)
}

fn check_authorization(
    pending: bool,
    expected: Option<&Value>,
    current: impl FnOnce() -> Result<Value, String>,
) -> Result<(), String> {
    if !pending {
        return Err("cancelled: App closed or request expired".into());
    }
    let manifest = current()?;
    if expected.is_some_and(|original| original != &manifest) {
        return Err("permission_denied: App admission changed during file selection".into());
    }
    Ok(())
}

struct Work {
    call: ServiceCall,
    reply: Replier,
    operation: Operation,
    manifest: Option<Value>,
    _reservation: Arc<Reservation>,
}
impl Work {
    fn authorized(&self) -> Result<(), String> {
        check_authorization(self.reply.is_pending(), self.manifest.as_ref(), || {
            admission(&self.call.app_id, self.operation.needs_storage())
        })
    }
    fn storage(&self) -> Result<StorageAccess, String> {
        self.authorized()?;
        splash_storage::storage_for_heap(self.reply.isolate_key(), &self.call.app_id)
            .ok_or("storage_unavailable: This request has no live app storage".into())
    }
}
struct Pending {
    id: LiveId,
    work: Work,
    export: Option<Vec<u8>>,
}
/// A selection a worker has staged beside the app's jail, with the reply
/// the UI sends once it links the file in.
struct Staged {
    work: Work,
    result: Result<(PreparedStorageImport, Value), String>,
}

pub fn register() {
    services::register_host_service(Box::new(FilesService));
}
struct FilesService;
impl HostService for FilesService {
    fn family(&self) -> &'static str {
        "files"
    }
    fn timeout(&self, call: &ServiceCall) -> Duration {
        if call.method() == "share" {
            Duration::from_secs(20)
        } else {
            TIMEOUT
        }
    }
    fn api_methods(&self) -> Vec<HostApiMethod> {
        let mut methods = vec![HostApiMethod::new("files.status", 1, "files", "Discover file transfer support without opening a dialog",
            json!({"type":"object","additionalProperties":false}),
            json!({"type":"object","required":["import_supported","export_supported","max_file_bytes","max_import_bytes","storage_granted","foreground_required"],
                "properties":{"import_supported":{"type":"boolean"},"export_supported":{"type":"boolean"},"max_file_bytes":{"type":"integer"},"max_import_bytes":{"type":"integer"},"photo_pick_supported":{"type":"boolean"},"text_share_supported":{"type":"boolean"},"max_share_text_bytes":{"type":"integer"},"storage_granted":{"type":"boolean"},"foreground_required":{"const":true}}}))
            .with_platforms(&["macos","windows","linux","android","ios","openharmony","web"]).with_agent_access(AgentAccess::Allowed)];
        if cfg!(target_os = "android") {
            methods.push(HostApiMethod::new("files.share", 1, "files", "Open Android's text share chooser; OS handoff does not confirm delivery",
                json!({"type":"object","required":["text"],"additionalProperties":false,"properties":{"text":{"type":"string","minLength":1,"maxLength":MAX_SHARE_TEXT}}}),
                json!({"type":"object","required":["handoff","delivery"],"properties":{"handoff":{"const":"chooser_opened"},"delivery":{"const":"unknown"}}}))
                .with_platforms(&["android"]).with_agent_access(AgentAccess::ForegroundOnly));
        }
        if !file_dialogs::native_file_bytes_supported() {
            return methods;
        }
        for (method, summary, input) in [
            (
                "import",
                "Select one document and copy it into a new app file; at most 64 MiB (16 MiB on Android) and the app's free storage, requires storage",
                json!({"type":"object","required":["path"],"additionalProperties":false,"properties":{"path":{"type":"string","minLength":1,"maxLength":2048}}}),
            ),
            (
                "pick_photo",
                "Choose one PNG, JPEG or WebP image into a new app file; at most 1 MiB, requires storage",
                json!({"type":"object","required":["path"],"additionalProperties":false,"properties":{"path":{"type":"string","minLength":1,"maxLength":2048}}}),
            ),
            (
                "export",
                "Save a snapshot of one app file to a native destination; requires storage",
                json!({"type":"object","required":["path"],"additionalProperties":false,"properties":{"path":{"type":"string","minLength":1,"maxLength":2048},"name":{"type":"string","minLength":1,"maxLength":128}}}),
            ),
        ] {
            methods.push(HostApiMethod::new(format!("files.{method}"), 1, "files", summary, input,
                json!({"type":"object","required":["cancelled"],"properties":{"cancelled":{"type":"boolean"},"path":{"type":"string"},"bytes":{"type":"integer","minimum":0},"mime":{"enum":["image/png","image/jpeg","image/webp"]}}}))
                .with_platforms(&["macos","windows","linux","android"]).with_agent_access(AgentAccess::ForegroundOnly));
        }
        methods
    }
    fn call(&mut self, call: ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
        if call.method() == "status" {
            if !call.args.as_object().is_some_and(|args| args.is_empty()) {
                reply.send(Err(
                    "invalid_arguments: status requires an empty object".into()
                ));
                return;
            }
            // Status is intentionally nonprompting and does not need storage.
            reply.send(admission(&call.app_id, false).map(|_| {
                json!({
                    "import_supported":file_dialogs::native_file_bytes_supported(),
                    "export_supported":file_dialogs::native_file_bytes_supported(),
                    "max_file_bytes":MAX_FILE_BYTES, "max_import_bytes":MAX_IMPORT, "foreground_required":true,
                    "photo_pick_supported":file_dialogs::native_file_bytes_supported(),
                    "text_share_supported":cfg!(target_os="android"), "max_share_text_bytes":MAX_SHARE_TEXT,
                    "storage_granted":crate::host_tools::script_apps::grants(&call.app_id,"storage")
                })
            }));
            return;
        }
        if !call.may_prompt || call.from_sheet {
            reply.send(Err(
                "foreground_required: Open the app to choose a file".into()
            ));
            return;
        }
        if call.method() == "share" && !cfg!(target_os = "android") {
            reply.send(Err(
                "unsupported_platform: Text sharing is available on Android only".into(),
            ));
            return;
        }
        if call.method() != "share" && !file_dialogs::native_file_bytes_supported() {
            reply.send(Err(
                "unsupported_platform: Native file byte transfer is unavailable on this host"
                    .into(),
            ));
            return;
        }
        let operation = match parse_operation(call.method(), &call.args) {
            Ok(op) => op,
            Err(e) => {
                reply.send(Err(e));
                return;
            }
        };
        let reservation = match Reservation::acquire() {
            Ok(r) => r,
            Err(e) => {
                reply.send(Err(e));
                return;
            }
        };
        // Dispatch holds the shared host-service registry lock: only queue here.
        *QUEUED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Work {
            call,
            reply,
            operation,
            manifest: None,
            _reservation: Arc::new(reservation),
        });
        SignalToUI::set_ui_signal();
    }
}

/// Called by the shell on every event, outside App Hub's service registry lock.
pub fn handle_event(cx: &mut Cx, event: &Event) {
    FOREGROUND.with(|foreground| foreground.borrow_mut().event(event));
    if cx.in_draw_event() {
        return;
    }
    commit_staged();
    PENDING.with(|slot| {
        if slot
            .borrow()
            .as_ref()
            .is_some_and(|p| !p.work.reply.is_pending())
        {
            slot.borrow_mut().take();
        }
        if let Event::AndroidIntegration { channel, payload } = event {
            if channel == "makepad.share_text.result" && payload.len() <= 2048 {
                if let Ok(value) = serde_json::from_str::<Value>(payload) {
                    let matched = slot.borrow().as_ref().is_some_and(|p| {
                        matches!(p.work.operation, Operation::Share { .. })
                            && value["id"].as_str() == Some(format!("{:016x}", p.id.0).as_str())
                    });
                    if matched {
                        if let Some(pending) = slot.borrow_mut().take() {
                            let result = pending
                                .work
                                .authorized()
                                .and_then(|_| share_outcome(&value));
                            pending.work.reply.send(result);
                        }
                    }
                }
            }
        }
        if let Event::Actions(actions) = event {
            for action in actions {
                let Some(action) = action.downcast_ref::<FileDialogAction>() else {
                    continue;
                };
                if slot.borrow().as_ref().is_some_and(|p| {
                    p.id == action.id() && !matches!(p.work.operation, Operation::Share { .. })
                }) {
                    if let Some(pending) = slot.borrow_mut().take() {
                        complete(cx, pending, action);
                    }
                }
            }
        }
        // Service dispatch may produce work off the UI thread. Never wait
        // for it here: the producer signals only after releasing this lock.
        // Its Reservation remains owned by the untouched queued work.
        let queued = match QUEUED.try_lock() {
            Ok(mut queued) => queued.take(),
            Err(TryLockError::Poisoned(error)) => error.into_inner().take(),
            Err(TryLockError::WouldBlock) => None,
        };
        if let Some(mut work) = queued {
            if !work.reply.is_pending() {
                return;
            }
            // may_prompt authenticates the request's origin, not its current
            // visibility. A queued call cannot open UI after suspension/switch.
            let foreground = FOREGROUND.with(|foreground| foreground.borrow().allows_launch(
                &work.call.app_id,
                work.call.may_prompt,
                makepad_widgets::splash_host::surface_is_foreground(work.reply.isolate_key(), &work.call.app_id),
            ));
            if !foreground {
                work.reply.send(Err("foreground_required: Return to this app before choosing or sharing a file".into()));
                return;
            }
            work.manifest = match admission(&work.call.app_id, work.operation.needs_storage()) {
                Ok(m) => Some(m),
                Err(e) => {
                    work.reply.send(Err(e));
                    return;
                }
            };
            let id = LiveId::unique();
            if let Operation::Share { text } = &work.operation {
                let result = work.authorized().and_then(|_| {
                    cx.share_text_request(&format!("{:016x}", id.0), text)
                        .map_err(String::from)
                });
                if let Err(error) = result {
                    work.reply.send(Err(error));
                    return;
                }
                *slot.borrow_mut() = Some(Pending {
                    id,
                    work,
                    export: None,
                });
                return;
            }
            let storage = match work.storage() {
                Ok(s) => s,
                Err(e) => {
                    work.reply.send(Err(e));
                    return;
                }
            };
            let reply = work.reply.clone();
            let app = work.call.app_id.clone();
            let manifest = work.manifest.clone();
            // The native loader can outlive Pending: a provider read already
            // in progress may block even after close/timeout revokes the reply.
            // Keep its one-transfer slot until the guard/worker is dropped.
            let reservation = work._reservation.clone();
            let dialog = FileDialog::new()
                .set_id(id)
                .set_multiple(false)
                .set_persistent_access(false)
                .set_access_guard(FileDialogAccessGuard::new(move || {
                    let _keep_slot = &reservation;
                    check_authorization(reply.is_pending(), manifest.as_ref(), || {
                        admission(&app, true)
                    })
                    .is_ok()
                }));
            let export = match &work.operation {
                Operation::Import { path } | Operation::PickPhoto { path } => {
                    if let Err(e) = storage.validate_path(path) {
                        work.reply.send(Err(e));
                        return;
                    }
                    let previous = cx.virtual_file_limits();
                    let limit = work.operation.selection_limit(storage.quota());
                    cx.set_virtual_file_limits(limit, limit);
                    let photo = matches!(work.operation, Operation::PickPhoto { .. });
                    let dialog = if photo {
                        dialog.add_filter(
                            "Images (PNG, JPEG, WebP)".into(),
                            vec!["png".into(), "jpg".into(), "jpeg".into(), "webp".into()],
                        )
                    } else {
                        dialog
                    };
                    cx.open_select_file_dialog(
                        dialog
                            .set_title(
                                if photo {
                                    "Choose an image (maximum 1 MiB)"
                                } else {
                                    "Import a file into this app"
                                }
                                .into(),
                            )
                            .want_bytes(true),
                    );
                    // begin() snapshots the limits for this dialog.
                    cx.set_virtual_file_limits(previous.max_file_size, previous.max_total_size);
                    None
                }
                Operation::Export { path, name } => {
                    let bytes = match storage.read_bytes(path) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            work.reply.send(Err(e));
                            return;
                        }
                    };
                    cx.open_save_file_dialog(
                        dialog
                            .set_title("Export a file from this app".into())
                            .set_filename(name.clone()),
                    );
                    Some(bytes)
                }
                Operation::Share { .. } => unreachable!("share dispatch is handled above"),
            };
            *slot.borrow_mut() = Some(Pending { id, work, export });
        }
    });
}

/// Links a staged selection into its app's live jail, between script turns.
/// Never waits for the worker's slot: a worker still filling it signals again.
fn commit_staged() {
    let staged = match STAGED.try_lock() {
        Ok(mut staged) => staged.take(),
        Err(TryLockError::Poisoned(error)) => error.into_inner().take(),
        Err(TryLockError::WouldBlock) => None,
    };
    if let Some(Staged { work, result }) = staged {
        let result = result.and_then(|(staged, value)| {
            work.storage()?.commit_import(staged)?;
            Ok(value)
        });
        work.reply.send(result);
    }
}

fn complete(cx: &mut Cx, pending: Pending, action: &FileDialogAction) {
    let Pending { work, export, .. } = pending;
    if matches!(
        action,
        FileDialogAction::FileCancelled { .. } | FileDialogAction::SaveFileCancelled { .. }
    ) {
        work.reply.send(Ok(json!({"cancelled":true})));
        return;
    }
    let storage = match work.storage() {
        Ok(s) => s,
        Err(e) => {
            work.reply.send(Err(e));
            return;
        }
    };
    match (&work.operation, action) {
        (
            Operation::Import { path } | Operation::PickPhoto { path },
            FileDialogAction::FileLoaded { files, .. },
        ) if files.len() == 1 => {
            let file = &files[0];
            let mime = if matches!(work.operation, Operation::PickPhoto { .. }) {
                match image_mime(&file.bytes) {
                    Ok(mime) => Some(mime),
                    Err(error) => {
                        work.reply.send(Err(error));
                        return;
                    }
                }
            } else {
                None
            };
            // A worker writes the bytes beside the jail; the UI links them in
            // between script turns (`commit_staged`), against the live jail and
            // its current quota.
            let path = path.clone();
            let bytes = file.bytes.clone();
            let snapshot = storage.worker_snapshot();
            let failed = work.reply.clone();
            let submitted = cx.task_pool().submit(Lane::Heavy, move || {
                let result = work
                    .authorized()
                    .and_then(|_| snapshot.prepare_import(&path, &bytes))
                    .map(|staged| {
                        let mut value = json!({"cancelled":false,"path":path,"bytes":bytes.len()});
                        if let Some(mime) = mime {
                            value["mime"] = mime.into();
                        }
                        (staged, value)
                    });
                *STAGED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Staged { work, result });
                SignalToUI::set_ui_signal();
            });
            match submitted {
                Ok(task) => task.detach(),
                Err(_) => failed.send(Err("busy: File worker is unavailable".into())),
            }
        }
        (
            Operation::Export { path, .. },
            FileDialogAction::SaveFileSelected {
                path: destination, ..
            },
        ) => {
            let Some(bytes) = export else {
                work.reply
                    .send(Err("export_unavailable: Missing snapshot".into()));
                return;
            };
            let path = path.clone();
            let destination = destination.clone();
            let failed = work.reply.clone();
            // The task owns the reservation, bounding provider IO and snapshots.
            let submitted = cx.task_pool().submit(Lane::Heavy, move || {
                let result = (|| {
                    work.authorized()?;
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        file_dialogs::write_selected_file_guarded(&destination, &bytes, || {
                            work.authorized().is_ok()
                        })?;
                        work.authorized()?;
                        Ok(json!({"cancelled":false,"path":path,"bytes":bytes.len()}))
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = (&destination, &bytes, &path);
                        Err("unsupported_platform: File export is unavailable".into())
                    }
                })();
                work.reply.send(result);
            });
            match submitted {
                Ok(task) => task.detach(),
                Err(_) => failed.send(Err("busy: File worker is unavailable".into())),
            }
        }
        (_, FileDialogAction::FileLoadFailed { .. }) => work.reply.send(Err(
            "read_failed: Selected document is unreadable or exceeds the byte limit".into(),
        )),
        _ => work.reply.send(Err(
            "invalid_selection: Expected one file for this dialog".into()
        )),
    }
}
