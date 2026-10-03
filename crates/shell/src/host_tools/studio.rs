//! Developer-only L0 glance rendering. The relay authorizes and audits the
//! call; this executor opens files relative to a trusted workspace descriptor.
//! No source/output path can select a different app, account or context.
use super::relay::{app_of_peer, SYSTEM};
use crate::ai_host::app_peers::host_tools::{
    CallerKind, HostToolCall, ToolExecutor, ToolOutcome, ToolReply,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const RENDER: &str = "studio.render";
pub const SUPPORTED: bool = cfg!(unix);
pub const SOURCE_MAX: usize = 16 * 1024;
pub const DATA_MAX: usize = 32 * 1024;
const MAX_WAIT: Duration = Duration::from_secs(25);
const MAX_IN_FLIGHT: usize = 2;
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
static SYSTEM_WORKSPACE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Called only with the kernel's session/open response by the system driver.
pub fn system_workspace_opened(path: Option<&str>) {
    *SYSTEM_WORKSPACE.lock().unwrap_or_else(|e| e.into_inner()) = path
        .filter(|p| Path::new(p).is_absolute())
        .and_then(|p| std::fs::canonicalize(p).ok());
}

pub fn declaration(app: &str) -> Value {
    json!({
        "name": RENDER, "app": app,
        "description": "Developer mode only. Render an L0 glance card at this device's actual glance width while Home is foreground. Read source_path and optional data_path from your own conversation workspace. Returns a PNG path for view_image; no bundle install or app execution. Rendering may take up to 20 seconds; the host call has its normal 30-second deadline.",
        "input_schema": {"type":"object", "properties": {
            "source_path":{"type":"string", "minLength":1,"maxLength":1024},
            "data_path":{"type":"string", "minLength":1,"maxLength":1024},
            "dark":{"type":"boolean"}
        }, "required":["source_path"],"additionalProperties":false},
        "output_schema": {"type":"object", "properties": {
            "path":{"type":"string"}, "width":{"type":"integer","minimum":1},
            "height":{"type":"integer","minimum":1}, "settled":{"type":"boolean"}
        }, "required":["path","width","height","settled"],"additionalProperties":false},
        "risk":"read", "background":false,"shareable":false
    })
}

fn workspace(call: &HostToolCall) -> Result<PathBuf, String> {
    if call.caller_kind == CallerKind::System {
        if call.calling_app != SYSTEM
            || call.peer.is_some()
            || call.context_id.is_some()
            || call.session_id != crate::system_chat::session::SYSTEM_SESSION
        {
            return Err("not the shell's system conversation".into());
        }
        return SYSTEM_WORKSPACE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| "the kernel has not confirmed the system workspace".into());
    }
    app_available(call)?;
    // A resumed peer can retain a kernel-owned cwd rather than the account
    // directory offered for new peers. Use only the broker's confirmed cwd;
    // agent_workspace() is a proposal for new peers, not their actual cwd.
    call.peer_workspace
        .clone()
        .filter(|p| p.is_absolute())
        .ok_or_else(|| "the broker has not confirmed this peer workspace".into())
}

fn app_available(call: &HostToolCall) -> Result<(), String> {
    let account = call
        .account
        .as_deref()
        .ok_or("the calling app has no account")?;
    if super::suspended(&call.calling_app, Some(account)) {
        return Err("the calling app account is suspended".into());
    }
    if let Some(why) = crate::app_storage::host()
        .and_then(|storage| super::workspace_refused_in(storage, &call.calling_app, account))
    {
        return Err(why);
    }
    Ok(())
}

fn authorized(app: &str, tag: &crate::dev_mode::DevTag) -> bool {
    crate::dev_mode::tag_valid(tag) && crate::dev_mode::grants_all(app)
}

#[derive(Default)]
pub struct StudioExecutor {
    running: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}
impl ToolExecutor for StudioExecutor {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        let root = match workspace(&call) {
            Ok(root) => root,
            Err(why) => {
                reply.finish(ToolOutcome::error("no_workspace", why));
                return;
            }
        };
        self.execute_scoped(call, reply, root);
    }
    fn cancel(&self, id: &str) {
        if let Some(flag) = self
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
        {
            flag.store(true, Ordering::SeqCst);
        }
        crate::studio::cancel(id);
    }
}
impl StudioExecutor {
    fn execute_scoped(&self, call: HostToolCall, reply: ToolReply, root: PathBuf) {
        let app = app_of_peer(&call.calling_app).to_string();
        let Some(tag) = crate::dev_mode::tag().filter(|t| authorized(&app, t)) else {
            reply.finish(ToolOutcome::error(
                "not_granted",
                "studio.render needs developer mode for the calling agent",
            ));
            return;
        };
        if call.app != app || call.name != RENDER {
            reply.finish(ToolOutcome::error(
                "not_granted",
                "studio.render belongs to the calling agent",
            ));
            return;
        }
        if let Err(why) = super::schema::check(&declaration(&app)["input_schema"], &call.args) {
            reply.finish(ToolOutcome::error("invalid_args", why));
            return;
        }
        crate::dev_mode::audit_tool_call(&app, RENDER, &call.args.to_string(), &call.calling_app);
        if IN_FLIGHT.fetch_add(1, Ordering::SeqCst) >= MAX_IN_FLIGHT {
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
            reply.finish(ToolOutcome::error(
                "busy",
                "two studio requests are already in flight",
            ));
            return;
        }
        let flag = Arc::new(AtomicBool::new(false));
        {
            let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
            if running.contains_key(&call.call_id) {
                IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
                reply.finish(ToolOutcome::error(
                    "busy",
                    "this studio call is already running",
                ));
                return;
            }
            running.insert(call.call_id.clone(), flag.clone());
        }
        let running = self.running.clone();
        std::thread::spawn(move || {
            let outcome = render_call(&call, &root, &app, &tag, &flag, &reply);
            reply.finish(outcome);
            running
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&call.call_id);
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

fn render_call(
    call: &HostToolCall,
    root: &Path,
    app: &str,
    tag: &crate::dev_mode::DevTag,
    cancel: &AtomicBool,
    reply: &ToolReply,
) -> ToolOutcome {
    #[cfg(not(unix))]
    {
        let _ = (call, root, app, tag, cancel, reply);
        ToolOutcome::error("unsupported", "studio rendering requires a Unix workspace")
    }
    #[cfg(unix)]
    {
        let run = || -> Result<Value, String> {
            let scope = scoped::Workspace::open(root, call.context_id.as_deref())?;
            let source = scope.read(call.args["source_path"].as_str().unwrap_or(""), SOURCE_MAX)?;
            let data = match call.args["data_path"].as_str() {
                Some(path) => serde_json::from_str(&scope.read(path, DATA_MAX)?)
                    .map_err(|e| format!("invalid data JSON: {e}"))?,
                None => json!({}),
            };
            let output = scope.output()?;
            if !authorized(app, tag) || cancel.load(Ordering::SeqCst) || !reply.is_open() {
                return Err("cancelled or developer mode revoked".into());
            }
            let (tx, rx) = std::sync::mpsc::channel();
            crate::studio::submit(crate::studio::RenderJob {
                id: call.call_id.clone(),
                app: app.to_string(),
                source,
                data,
                dark: call.args["dark"].as_bool().unwrap_or(false),
                width: None,
                output: output.file.try_clone().map_err(|e| e.to_string())?,
                reply: tx,
                dev_tag: tag.clone(),
            })?;
            if call.call_id.starts_with("studio-test-") {
                makepad_widgets::log!("studio-test-started: {}", call.call_id);
            }
            let timeout =
                Duration::from_millis(call.timeout_ms.saturating_sub(500).max(1)).min(MAX_WAIT);
            let deadline = Instant::now() + timeout;
            let result = loop {
                if cancel.load(Ordering::SeqCst) || !reply.is_open() || !authorized(app, tag) {
                    crate::studio::cancel(&call.call_id);
                    break Err("cancelled or developer mode revoked".to_string());
                }
                if Instant::now() >= deadline {
                    crate::studio::cancel(&call.call_id);
                    break Err("studio render timed out".to_string());
                }
                match rx.recv_timeout(Duration::from_millis(25)) {
                    Ok(value) => break value,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => break Err("studio renderer stopped".into()),
                }
            }?;
            if !authorized(app, tag) || cancel.load(Ordering::SeqCst) || !reply.is_open() {
                return Err("cancelled or developer mode revoked".into());
            }
            if call.caller_kind == CallerKind::AppPeer {
                app_available(call)?;
            }
            scope.verify(root, call.context_id.as_deref())?;
            // Pinned octos resolves a peer-context session to binding.cwd,
            // <peer cwd>/contexts/<id> (app_binding::resolve_session_app_binding).
            // Its view_image is rooted there, not at the parent peer workspace.
            let path = output.keep();
            Ok(
                json!({"path":path,"width":result.width,"height":result.height,"settled":result.settled}),
            )
        };
        match run() {
            Ok(data) => ToolOutcome::Ok(data),
            Err(why) => ToolOutcome::error("studio_render_failed", why),
        }
    }
}

/// Explicit developer test action only. Its workspace comes from a local
/// launch fixture, never from model tool arguments. No provider is required.
pub fn test_action(spec_path: &str) -> Result<(), String> {
    let home = crate::octosense::paths::home();
    makepad_widgets::log!("studio-test-profile: home={} canonical={} build={:?}",
        home.display(), std::fs::canonicalize(&home).unwrap_or(home.clone()).display(),
        crate::dev_mode::BuildKind::current());
    if !crate::dev_mode::grants_all(SYSTEM) {
        return Err("studio test action requires developer mode for system".into());
    }
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(spec_path)
        .map_err(|e| e.to_string())?
        .take(8193)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > 8192 {
        return Err("studio test fixture is too large".into());
    }
    let mut args: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let root = PathBuf::from(
        args["workspace"]
            .as_str()
            .ok_or("fixture needs workspace")?,
    );
    if !root.is_absolute() {
        return Err("fixture workspace must be absolute".into());
    }
    args.as_object_mut()
        .ok_or("fixture must be an object")?
        .remove("workspace");
    let id = format!("studio-test-{}", uuid::Uuid::new_v4());
    let mut call = HostToolCall::parse(&json!({"call_id":id,"name":RENDER,"app":SYSTEM,"session_id":crate::system_chat::session::SYSTEM_SESSION,
        "caller":{"kind":"system"},"args":args,"timeout_ms":30_000,"risk":"read"})).map_err(|e| e.to_string())?;
    call.calling_app = SYSTEM.into();
    #[cfg(unix)]
    let result_file = scoped::Workspace::open(&root, None)?.result_file()?;
    let reply = ToolReply::new(id, move |fields| {
        #[cfg(unix)]
        {
            use std::io::Write;
            let data = fields.to_string();
            if data.len() <= 8192 {
                let mut file = &result_file;
                if let Err(error) = file
                    .write_all(data.as_bytes())
                    .and_then(|_| file.sync_all())
                {
                    makepad_widgets::log!("studio-test-result-file-error: {}", error);
                }
            }
        }
        makepad_widgets::log!("studio-test-result: {}", fields)
    });
    StudioExecutor::default().execute_scoped(call, reply, root);
    Ok(())
}

#[cfg(unix)]
mod scoped {
    use super::*;
    use std::ffi::{CString, OsStr};
    use std::fs::File;
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::Component;

    pub struct Workspace {
        dir: Arc<OwnedFd>,
        identity: (u64, u64),
    }
    fn cstr(s: &OsStr) -> Result<CString, String> {
        CString::new(s.as_bytes()).map_err(|_| "NUL in path".into())
    }
    fn openat(dir: i32, name: &OsStr, flags: i32) -> Result<OwnedFd, String> {
        let c = cstr(name)?;
        let fd = unsafe {
            libc::openat(
                dir,
                c.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
    fn parts(path: &str) -> Result<Vec<&OsStr>, String> {
        let parts: Result<Vec<_>, _> = Path::new(path)
            .components()
            .map(|p| match p {
                Component::Normal(n) => Ok(n),
                _ => Err("path must stay relative to your own workspace".to_string()),
            })
            .collect();
        let parts = parts?;
        if parts.is_empty() {
            return Err("empty path".into());
        }
        if parts[0].to_string_lossy().eq_ignore_ascii_case("contexts") {
            return Err("other conversation folders are not inputs".into());
        }
        Ok(parts)
    }
    impl Workspace {
        pub fn open(root: &Path, context: Option<&str>) -> Result<Self, String> {
            let mut fd = openat(
                libc::AT_FDCWD,
                root.as_os_str(),
                libc::O_RDONLY | libc::O_DIRECTORY,
            )?;
            if let Some(context) = context {
                let component = parts(context)?;
                if component.len() != 1 {
                    return Err("invalid trusted context id".into());
                }
                fd = openat(
                    fd.as_raw_fd(),
                    OsStr::new("contexts"),
                    libc::O_RDONLY | libc::O_DIRECTORY,
                )?;
                fd = openat(
                    fd.as_raw_fd(),
                    component[0],
                    libc::O_RDONLY | libc::O_DIRECTORY,
                )?;
            }
            let file = File::from(fd);
            let m = file.metadata().map_err(|e| e.to_string())?;
            Ok(Self {
                identity: (m.dev(), m.ino()),
                dir: Arc::new(file.into()),
            })
        }
        pub fn verify(&self, root: &Path, context: Option<&str>) -> Result<(), String> {
            if Self::open(root, context)?.identity != self.identity {
                return Err("workspace changed during render".into());
            }
            Ok(())
        }
        pub fn read(&self, path: &str, cap: usize) -> Result<String, String> {
            let names = parts(path)?;
            let mut dir = self.dir.clone();
            for name in &names[..names.len() - 1] {
                dir = Arc::new(openat(
                    dir.as_raw_fd(),
                    name,
                    libc::O_RDONLY | libc::O_DIRECTORY,
                )?);
            }
            let mut file = File::from(openat(
                dir.as_raw_fd(),
                names[names.len() - 1],
                libc::O_RDONLY,
            )?);
            let meta = file.metadata().map_err(|e| e.to_string())?;
            if !meta.is_file() || meta.len() > cap as u64 {
                return Err("input must be a bounded regular file".into());
            }
            let mut out = String::new();
            (&mut file)
                .take(cap as u64 + 1)
                .read_to_string(&mut out)
                .map_err(|e| e.to_string())?;
            if out.len() > cap {
                return Err("input exceeds render limit".into());
            }
            Ok(out)
        }
        pub fn result_file(&self) -> Result<File, String> {
            openat(
                self.dir.as_raw_fd(),
                OsStr::new("studio-result.json"),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            )
            .map(File::from)
        }
        pub fn output(&self) -> Result<Output, String> {
            let name = format!(".studio-{}.png", uuid::Uuid::new_v4());
            let fd = openat(
                self.dir.as_raw_fd(),
                OsStr::new(&name),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            )?;
            Ok(Output {
                file: File::from(fd),
                dir: self.dir.clone(),
                name,
                keep: false,
            })
        }
    }
    pub struct Output {
        pub file: File,
        dir: Arc<OwnedFd>,
        name: String,
        keep: bool,
    }
    impl Output {
        pub fn keep(mut self) -> String {
            self.keep = true;
            self.name.clone()
        }
    }
    impl Drop for Output {
        fn drop(&mut self) {
            if !self.keep {
                if let Ok(name) = CString::new(self.name.clone()) {
                    unsafe {
                        libc::unlinkat(self.dir.as_raw_fd(), name.as_ptr(), 0);
                    }
                }
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn context_inputs_and_outputs_stay_with_the_caller() {
        let temp = crate::app_storage::tests::Scratch::new("studio");
        let root = temp.0.as_path();
        std::fs::create_dir_all(root.join("contexts/a")).unwrap();
        std::fs::create_dir_all(root.join("contexts/b")).unwrap();
        std::fs::write(root.join("contexts/a/card.l0"), "own").unwrap();
        std::fs::write(root.join("contexts/b/card.l0"), "other").unwrap();
        let scope = scoped::Workspace::open(root, Some("a")).unwrap();
        assert_eq!(scope.read("card.l0", SOURCE_MAX).unwrap(), "own");
        for path in ["../b/card.l0", "/etc/passwd", "../../card.l0"] {
            assert!(scope.read(path, SOURCE_MAX).is_err(), "{path}");
        }
        assert!(scoped::Workspace::open(root, Some("../b")).is_err());
        let output = scope.output().unwrap();
        let name = output.keep();
        assert!(root.join("contexts/a").join(&name).is_file());
        assert!(!root.join(&name).exists());
        assert!(!root.join("contexts/b").join(&name).exists());
        let peer = scoped::Workspace::open(root, None).unwrap();
        assert!(peer.read("contexts/b/card.l0", SOURCE_MAX).is_err());
        assert!(peer.read("Contexts/b/card.l0", SOURCE_MAX).is_err());
    }

    #[test]
    fn inputs_reject_links_devices_and_oversized_data() {
        let temp = crate::app_storage::tests::Scratch::new("studio");
        let outside = crate::app_storage::tests::Scratch::new("studio");
        std::fs::write(outside.0.as_path().join("private"), "private").unwrap();
        symlink(outside.0.as_path(), temp.0.as_path().join("jump")).unwrap();
        symlink(
            outside.0.as_path().join("private"),
            temp.0.as_path().join("file-link"),
        )
        .unwrap();
        symlink("/dev/zero", temp.0.as_path().join("device")).unwrap();
        std::fs::write(temp.0.as_path().join("large"), vec![b'x'; DATA_MAX + 1]).unwrap();
        let scope = scoped::Workspace::open(temp.0.as_path(), None).unwrap();
        for path in ["jump/private", "file-link", "device", "large"] {
            assert!(scope.read(path, DATA_MAX).is_err(), "{path}");
        }
        std::fs::create_dir_all(temp.0.as_path().join("contexts")).unwrap();
        symlink(outside.0.as_path(), temp.0.as_path().join("contexts/other")).unwrap();
        assert!(scoped::Workspace::open(temp.0.as_path(), Some("other")).is_err());
    }

    #[test]
    fn failed_outputs_are_unlinked_and_workspace_replacement_is_detected() {
        let temp = crate::app_storage::tests::Scratch::new("studio");
        let root = temp.0.as_path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let scope = scoped::Workspace::open(&root, None).unwrap();
        {
            let _output = scope.output().unwrap();
        }
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::rename(&root, temp.0.as_path().join("old")).unwrap();
        std::fs::create_dir(&root).unwrap();
        assert!(scope.verify(&root, None).is_err());
    }

    #[test]
    fn schema_does_not_accept_workspace_output_or_bundle_paths() {
        let schema = declaration("system")["input_schema"].clone();
        assert!(super::super::schema::check(
            &schema,
            &json!({"source_path":"main.card","dark":true})
        )
        .is_ok());
        for extra in [
            "workspace",
            "app",
            "context_id",
            "output",
            "bundle",
            "width",
        ] {
            let mut args = json!({"source_path":"main.card"});
            args[extra] = json!("untrusted");
            assert!(
                super::super::schema::check(&schema, &args).is_err(),
                "{extra}"
            );
        }
    }

    #[test]
    fn cancellation_marks_the_running_call() {
        let executor = StudioExecutor::default();
        let flag = Arc::new(AtomicBool::new(false));
        let id = format!("cancel-test-{}", uuid::Uuid::new_v4());
        executor
            .running
            .lock()
            .unwrap()
            .insert(id.clone(), flag.clone());
        executor.cancel(&id);
        assert!(flag.load(Ordering::SeqCst));
    }
}
