//! Foreground, single-file transfer between native dialogs and the live app's
//! existing Splash jail. Neither a selected host path nor a provider URI crosses
//! the script boundary. See README.md for the public contract and integration.
use makepad_widgets::{
    makepad_platform::{
        file_dialogs::{self, FileDialog, FileDialogAction},
        thread::Lane,
        SignalToUI,
    },
    splash_storage::{self, StorageAccess, MAX_FILE_BYTES},
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
        Mutex,
    },
    time::Duration,
};

#[cfg(test)]
mod tests;

const TIMEOUT: Duration = Duration::from_secs(300);
static BUSY: AtomicBool = AtomicBool::new(false);
static QUEUED: Mutex<Option<Work>> = Mutex::new(None);
thread_local! { static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) }; }

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
    Export { path: String, name: String },
}

fn parse_operation(method: &str, args: &Value) -> Result<Operation, String> {
    let args = args
        .as_object()
        .ok_or("invalid_arguments: Expected an object")?;
    let export = match method {
        "import" => false,
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
        return Ok(Operation::Import { path });
    }
    let name = match args.get("name") {
        Some(value) => value
            .as_str()
            .ok_or("invalid_arguments: name must be a basename")?,
        None => path.rsplit(['/', '\\']).next().unwrap_or(""),
    };
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
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

struct Work {
    call: ServiceCall,
    reply: Replier,
    operation: Operation,
    manifest: Option<Value>,
    _reservation: Reservation,
}
impl Work {
    fn storage(&self) -> Result<StorageAccess, String> {
        if !self.reply.is_pending() {
            return Err("cancelled: App closed or request expired".into());
        }
        let manifest = admission(&self.call.app_id, true)?;
        if self
            .manifest
            .as_ref()
            .is_some_and(|original| original != &manifest)
        {
            return Err("permission_denied: App admission changed during file selection".into());
        }
        splash_storage::storage_for_heap(self.reply.isolate_key(), &self.call.app_id)
            .ok_or("storage_unavailable: This request has no live app storage".into())
    }
}
struct Pending {
    id: LiveId,
    work: Work,
    export: Option<Vec<u8>>,
}

pub fn register() {
    services::register_host_service(Box::new(FilesService));
}
struct FilesService;
impl HostService for FilesService {
    fn family(&self) -> &'static str {
        "files"
    }
    fn timeout(&self, _: &ServiceCall) -> Duration {
        TIMEOUT
    }
    fn api_methods(&self) -> Vec<HostApiMethod> {
        let mut methods = vec![HostApiMethod::new("files.status", 1, "files", "Discover file transfer support without opening a dialog",
            json!({"type":"object","additionalProperties":false}),
            json!({"type":"object","required":["import_supported","export_supported","max_file_bytes","storage_granted","foreground_required"],
                "properties":{"import_supported":{"type":"boolean"},"export_supported":{"type":"boolean"},"max_file_bytes":{"type":"integer"},"storage_granted":{"type":"boolean"},"foreground_required":{"const":true}}}))
            .with_platforms(&["macos","windows","linux","android","ios","openharmony","web"]).with_agent_access(AgentAccess::Allowed)];
        if !file_dialogs::native_file_bytes_supported() {
            return methods;
        }
        for (method, summary, input) in [
            (
                "import",
                "Select one document and copy it into a new app file; requires storage",
                json!({"type":"object","required":["path"],"additionalProperties":false,"properties":{"path":{"type":"string","minLength":1,"maxLength":2048}}}),
            ),
            (
                "export",
                "Save a snapshot of one app file to a native destination; requires storage",
                json!({"type":"object","required":["path"],"additionalProperties":false,"properties":{"path":{"type":"string","minLength":1,"maxLength":2048},"name":{"type":"string","minLength":1,"maxLength":128}}}),
            ),
        ] {
            methods.push(HostApiMethod::new(format!("files.{method}"), 1, "files", summary, input,
                json!({"type":"object","required":["cancelled"],"properties":{"cancelled":{"type":"boolean"},"path":{"type":"string"},"bytes":{"type":"integer","minimum":0}}}))
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
                    "max_file_bytes":MAX_FILE_BYTES, "foreground_required":true,
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
        if !file_dialogs::native_file_bytes_supported() {
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
            _reservation: reservation,
        });
        SignalToUI::set_ui_signal();
    }
}

/// Called by the shell on every event, outside App Hub's service registry lock.
pub fn handle_event(cx: &mut Cx, event: &Event) {
    if cx.in_draw_event() {
        return;
    }
    PENDING.with(|slot| {
        if slot
            .borrow()
            .as_ref()
            .is_some_and(|p| !p.work.reply.is_pending())
        {
            slot.borrow_mut().take();
        }
        if let Event::Actions(actions) = event {
            for action in actions {
                let Some(action) = action.downcast_ref::<FileDialogAction>() else {
                    continue;
                };
                if slot.borrow().as_ref().is_some_and(|p| p.id == action.id()) {
                    if let Some(pending) = slot.borrow_mut().take() {
                        complete(cx, pending, action);
                    }
                }
            }
        }
        let queued = QUEUED.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(mut work) = queued {
            if !work.reply.is_pending() {
                return;
            }
            let storage = match work.storage() {
                Ok(s) => s,
                Err(e) => {
                    work.reply.send(Err(e));
                    return;
                }
            };
            work.manifest = match admission(&work.call.app_id, true) {
                Ok(m) => Some(m),
                Err(e) => {
                    work.reply.send(Err(e));
                    return;
                }
            };
            let id = LiveId::unique();
            let dialog = FileDialog::new()
                .set_id(id)
                .set_multiple(false)
                .set_persistent_access(false);
            let export = match &work.operation {
                Operation::Import { path } => {
                    if let Err(e) = storage.validate_path(path) {
                        work.reply.send(Err(e));
                        return;
                    }
                    let previous = cx.virtual_file_limits();
                    let limit = MAX_FILE_BYTES.min(storage.quota());
                    cx.set_virtual_file_limits(limit, limit);
                    cx.open_select_file_dialog(
                        dialog
                            .set_title("Import a file into this app".into())
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
            };
            *slot.borrow_mut() = Some(Pending { id, work, export });
        }
    });
}

fn complete(cx: &mut Cx, pending: Pending, action: &FileDialogAction) {
    let Pending { work, export, .. } = pending;
    let storage = match work.storage() {
        Ok(s) => s,
        Err(e) => {
            work.reply.send(Err(e));
            return;
        }
    };
    match (&work.operation, action) {
        (
            _,
            FileDialogAction::FileCancelled { .. } | FileDialogAction::SaveFileCancelled { .. },
        ) => work.reply.send(Ok(json!({"cancelled":true}))),
        (Operation::Import { path }, FileDialogAction::FileLoaded { files, .. })
            if files.len() == 1 =>
        {
            let file = &files[0];
            // Uses the current live isolate's jail and granted quota, and runs
            // between script turns so quota checking and writing are serialized.
            work.reply.send(
                storage
                    .import_new(path, &file.bytes)
                    .map(|_| json!({"cancelled":false,"path":path,"bytes":file.bytes.len()})),
            );
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
                let _reservation = work._reservation;
                if !work.reply.is_pending() {
                    return;
                }
                let result = admission(&work.call.app_id, true).and_then(|manifest| {
                    if work.manifest.as_ref() != Some(&manifest) {
                        return Err("permission_denied: App admission changed".into());
                    }
                    if !work.reply.is_pending() {
                        return Err("cancelled: App closed or request expired".into());
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    file_dialogs::write_selected_file(&destination, &bytes)?;
                    #[cfg(target_arch = "wasm32")]
                    return Err("unsupported_platform: File export is unavailable".into());
                    Ok(json!({"cancelled":false,"path":path,"bytes":bytes.len()}))
                });
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
