//! Positive control for the native message observer, in this unpublished test
//! binary only. It never shares a controller, HWND, profile, or page with the
//! production browser. The fixed document has no network access or host objects.
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use webview2_windows::{
    core::{w, Interface, Result as WinResult, BOOL, PCWSTR},
    Win32::{
        Foundation::{E_FAIL, HWND, RECT},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
};

static NEXT_PROBE: AtomicU64 = AtomicU64::new(1);
const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);
const DOCUMENT: &str = r#"<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'"><title>Native message observer positive control</title><script>chrome.webview.postMessage('octosense-synthetic-positive-control');</script>"#;

type Shared = Rc<RefCell<State>>;

pub struct ProbeOutcome {
    pub passed: bool,
    pub delivered_messages: u64,
    pub web_message_enabled: Option<bool>,
    pub host_objects_allowed: Option<bool>,
    pub cleanup_complete: bool,
    pub error: Option<String>,
}

struct State {
    child: HWND,
    controller: Option<ICoreWebView2Controller>,
    webview: Option<ICoreWebView2>,
    message_token: Option<i64>,
    closed: bool,
    creation_pending: bool,
    delivered_messages: u64,
    web_message_enabled: Option<bool>,
    host_objects_allowed: Option<bool>,
    error: Option<String>,
}

/// Must be started, polled, and dropped on the fixture's existing UI STA.
/// `Rc` intentionally prevents moving this controller onto another thread.
pub struct WindowsMessageProbe {
    state: Shared,
    folder: PathBuf,
    started: Instant,
    cleanup_started: Option<Instant>,
    reported: bool,
}

impl WindowsMessageProbe {
    pub fn start() -> std::result::Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let folder = std::env::temp_dir().join(format!(
            "octosense-web-message-probe-{}-{stamp}-{}",
            std::process::id(),
            NEXT_PROBE.fetch_add(1, Ordering::Relaxed)
        ));
        // Create a new directory, never reuse an existing browser profile.
        std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
        let child = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("OctoSense synthetic message probe"),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
                0,
                0,
                320,
                240,
                None,
                None,
                GetModuleHandleW(None).ok().map(Into::into),
                None,
            )
        };
        let child = match child {
            Ok(child) => child,
            Err(error) => {
                let _ = std::fs::remove_dir(&folder);
                return Err(error.to_string());
            }
        };
        // WS_VISIBLE is deliberately absent: this is an owned hidden test HWND.
        let state = Rc::new(RefCell::new(State {
            child,
            controller: None,
            webview: None,
            message_token: None,
            closed: false,
            creation_pending: true,
            delivered_messages: 0,
            web_message_enabled: None,
            host_objects_allowed: None,
            error: None,
        }));
        let probe = Self {
            state,
            folder,
            started: Instant::now(),
            cleanup_started: None,
            reported: false,
        };
        if let Err(error) = unsafe { probe.start_environment() } {
            return Err(error.to_string()); // Drop closes the owned window.
        }
        Ok(probe)
    }

    /// Returns exactly one terminal result. All work is asynchronous; the
    /// caller keeps the normal UI message loop running between polls.
    pub fn poll(&mut self) -> Option<ProbeOutcome> {
        if self.reported {
            return None;
        }
        let finish = {
            let mut state = self.state.borrow_mut();
            if self.started.elapsed() >= LOAD_TIMEOUT && state.delivered_messages == 0 {
                state
                    .error
                    .get_or_insert_with(|| "Native positive-control delivery timed out".into());
            }
            state.delivered_messages > 0 || state.error.is_some()
        };
        if finish && self.cleanup_started.is_none() {
            self.close();
            self.cleanup_started = Some(Instant::now());
        }
        let cleanup_started = self.cleanup_started?;
        // Closing WebView2 releases its user-data directory asynchronously.
        // Retry only while the UI timer is running; never sleep or block COM.
        // A timed-out create operation may still be completing. Its weak
        // callback closes late controllers; do not claim profile cleanup until
        // it has settled and cannot recreate this directory.
        let cleanup_complete = !self.state.borrow().creation_pending
            && match std::fs::remove_dir_all(&self.folder) {
                Ok(()) => true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                Err(_) => false,
            };
        if !cleanup_complete && cleanup_started.elapsed() < CLEANUP_TIMEOUT {
            return None;
        }
        let mut state = self.state.borrow_mut();
        if !cleanup_complete {
            state
                .error
                .get_or_insert_with(|| "Synthetic WebView2 profile cleanup timed out".into());
        }
        self.reported = true;
        Some(ProbeOutcome {
            passed: state.error.is_none()
                && state.delivered_messages > 0
                && state.web_message_enabled == Some(true)
                && state.host_objects_allowed == Some(false)
                && cleanup_complete,
            delivered_messages: state.delivered_messages,
            web_message_enabled: state.web_message_enabled,
            host_objects_allowed: state.host_objects_allowed,
            cleanup_complete,
            error: state.error.clone(),
        })
    }

    unsafe fn start_environment(&self) -> WinResult<()> {
        let options: ICoreWebView2EnvironmentOptions =
            CoreWebView2EnvironmentOptions::default().into();
        options.SetAllowSingleSignOnUsingOSPrimaryAccount(false)?;
        let folder = wide(&self.folder.to_string_lossy());
        let weak = Rc::downgrade(&self.state);
        CreateCoreWebView2EnvironmentWithOptions(
            PCWSTR::null(),
            PCWSTR(folder.as_ptr()),
            &options,
            &CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
                move |result, environment| {
                    let Some(state) = weak.upgrade() else {
                        return Ok(());
                    };
                    if state.borrow().closed {
                        state.borrow_mut().creation_pending = false;
                        return Ok(());
                    }
                    let result = result
                        .and_then(|_| environment.ok_or_else(|| E_FAIL.into()))
                        .and_then(|environment| create_controller(&state, &environment));
                    if let Err(error) = result {
                        let mut state = state.borrow_mut();
                        state.creation_pending = false;
                        state.error = Some(error.to_string());
                    }
                    Ok(())
                },
            )),
        )
    }

    fn close(&mut self) {
        let (controller, webview, token, child) = {
            let mut state = self.state.borrow_mut();
            state.closed = true;
            (
                state.controller.take(),
                state.webview.take(),
                state.message_token.take(),
                std::mem::take(&mut state.child),
            )
        };
        unsafe {
            if let (Some(webview), Some(token)) = (&webview, token) {
                let _ = webview.remove_WebMessageReceived(token);
            }
            drop(webview);
            if let Some(controller) = controller {
                close_controller(&controller);
            }
            if !child.is_invalid() {
                let _ = DestroyWindow(child);
            }
        }
    }
}

impl Drop for WindowsMessageProbe {
    fn drop(&mut self) {
        self.close();
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

unsafe fn create_controller(
    state: &Shared,
    environment: &ICoreWebView2Environment,
) -> WinResult<()> {
    let version: ICoreWebView2Environment10 = environment.cast()?;
    let options = version.CreateCoreWebView2ControllerOptions()?;
    options.SetIsInPrivateModeEnabled(true)?;
    options.SetProfileName(w!("synthetic-message-probe"))?;
    let weak = Rc::downgrade(state);
    let child = state.borrow().child;
    version.CreateCoreWebView2ControllerWithOptions(
        child,
        &options,
        &CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
            move |result, controller| {
                let Some(state) = weak.upgrade() else {
                    if let Some(controller) = controller {
                        close_controller(&controller);
                    }
                    return Ok(());
                };
                state.borrow_mut().creation_pending = false;
                if state.borrow().closed {
                    if let Some(controller) = controller {
                        close_controller(&controller);
                    }
                    return Ok(());
                }
                let result = result
                    .and_then(|_| controller.ok_or_else(|| E_FAIL.into()))
                    .and_then(|controller| {
                        let result = configure(&state, &controller);
                        if result.is_err() {
                            close_controller(&controller);
                        }
                        result
                    });
                if let Err(error) = result {
                    state.borrow_mut().error = Some(error.to_string());
                }
                Ok(())
            },
        )),
    )
}

unsafe fn configure(state: &Shared, controller: &ICoreWebView2Controller) -> WinResult<()> {
    let webview = controller.CoreWebView2()?;
    let settings = webview.Settings()?;
    settings.SetAreHostObjectsAllowed(false)?;
    // The only enablement is this synthetic, unpublished test controller.
    settings.SetIsWebMessageEnabled(true)?;
    settings.SetAreDefaultScriptDialogsEnabled(false)?;
    settings.SetAreDefaultContextMenusEnabled(false)?;
    settings.SetAreDevToolsEnabled(false)?;
    let mut messaging = BOOL::default();
    let mut host_objects = BOOL::default();
    settings.IsWebMessageEnabled(&mut messaging)?;
    settings.AreHostObjectsAllowed(&mut host_objects)?;
    {
        let mut state = state.borrow_mut();
        state.web_message_enabled = Some(messaging.as_bool());
        state.host_objects_allowed = Some(host_objects.as_bool());
    }
    if !messaging.as_bool() || host_objects.as_bool() {
        return Err(E_FAIL.into());
    }
    let weak = Rc::downgrade(state);
    let mut message_token = 0;
    webview.add_WebMessageReceived(
        &WebMessageReceivedEventHandler::create(Box::new(move |_, _| {
            // Counting alone proves the native COM event works. Never read,
            // decode, dispatch, or echo a page-supplied message.
            if let Some(state) = weak.upgrade() {
                let mut state = state.borrow_mut();
                if !state.closed {
                    state.delivered_messages = state.delivered_messages.saturating_add(1);
                }
            }
            Ok(())
        })),
        &mut message_token,
    )?;
    // Even this test controller refuses page popups and device grants.
    let mut token = 0;
    webview.add_NewWindowRequested(
        &NewWindowRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                args.SetHandled(true)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    webview.add_PermissionRequested(
        &PermissionRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    controller.SetBounds(RECT {
        left: 0,
        top: 0,
        right: 320,
        bottom: 240,
    })?;
    controller.SetIsVisible(false)?;
    {
        let mut state = state.borrow_mut();
        state.controller = Some(controller.clone());
        state.webview = Some(webview.clone());
        state.message_token = Some(message_token);
    }
    let html = wide(DOCUMENT);
    webview.NavigateToString(PCWSTR(html.as_ptr()))
}

unsafe fn close_controller(controller: &ICoreWebView2Controller) {
    if let Ok(profile) = controller
        .CoreWebView2()
        .and_then(|view| view.cast::<ICoreWebView2_13>())
        .and_then(|view| view.Profile())
        .and_then(|profile| profile.cast::<ICoreWebView2Profile8>())
    {
        let _ = profile.Delete();
    }
    let _ = controller.Close();
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
