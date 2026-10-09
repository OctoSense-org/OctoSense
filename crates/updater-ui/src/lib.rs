//! Host-owned release UI. No agent tools or contained-app service can install an update.
use makepad_app_module::{
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, OpenSchema, ServiceExecutor,
    ValidatedOpen,
};
pub use makepad_widgets;
use makepad_widgets::*;
use octosense_updater::{
    BuildIdentity, Cancellation, Channel, CheckResult, CheckStatus, Client, Offer, Platform,
    Product, VerifiedArtifact,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Mutex, OnceLock,
    },
};

mod install;

static CACHE: OnceLock<PathBuf> = OnceLock::new();
static BUSY: AtomicBool = AtomicBool::new(false);
static REQUEST: AtomicU64 = AtomicU64::new(1);
static ANDROID: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());

/// The shell supplies a host-private directory, outside every app's storage jail.
pub fn configure(cache: PathBuf) {
    let _ = CACHE.set(cache);
}

/// Android integration packets are consumed by the shell before module dispatch.
pub fn android_result(payload: &str) {
    if payload.len() > 8192 {
        return;
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
        let mut pending = ANDROID.lock().unwrap_or_else(|e| e.into_inner());
        if pending.len() == 32 {
            pending.remove(0);
        }
        pending.push(value);
        makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
    }
}

script_mod! {
    use mod.prelude.widgets.*
    mod.widgets.UpdaterView = set_type_default() do #(UpdaterView::register_widget(vm)) {
        ..mod.widgets.RectView
        width: Fill height: Fill
        draw_bg.color: theme.color_bg_app
        flow: Down padding: 24 spacing: 18
        title := Label { text: "OctoSense Updates" draw_text.text_style.font_size: 25 }
        current := Label { width: Fill text: "Reading this installation…" draw_text.wrap: Words }
        ScrollYView {
            width: Fill height: Fill
            body := View {
                width: Fill height: Fit flow: Down spacing: 16
                channel := Button { height: 44 text: "Channel: Preview" }
                source := Label { width: Fill text: "Official releases from OctoSense-org / OctoSense on GitHub." draw_text.wrap: Words }
                status := Label { width: Fill text: "Check for an update when you are ready." draw_text.wrap: Words }
                offer := Label { width: Fill text: "" draw_text.wrap: Words }
                check := Button { width: Fill height: 44 text: "Check for updates" }
                download := Button { width: Fill height: 44 text: "Download update" visible: false }
                cancel := Button { width: Fill height: 44 text: "Cancel download" visible: false }
                install := Button { width: Fill height: 44 text: "Open installer" visible: false }
                permission := Button { width: Fill height: 44 text: "Allow updates in Android Settings" visible: false }
                retry_install := Button { width: Fill height: 44 text: "Refresh installation status" visible: false }
                notes := Button { width: Fill height: 44 text: "Release notes" visible: false }
                detail := Label { width: Fill text: "Updates are downloaded and verified before you choose to install. Save your work before replacing or restarting OctoSense." draw_text.wrap: Words }
            }
        }
    }
}

enum Completed {
    Checked(CheckResult),
    Downloaded(VerifiedArtifact),
    Opened(String),
    AndroidReady(PathBuf, String),
}
enum WorkerEvent {
    Progress(u64, u64),
    Finished(Box<Result<Completed, String>>),
}
struct Worker {
    receive: mpsc::Receiver<WorkerEvent>,
    cancel: Cancellation,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn worker(
    run: impl FnOnce(&Cancellation, &mpsc::SyncSender<WorkerEvent>) -> Result<Completed, String>
        + Send
        + 'static,
) -> Result<Worker, String> {
    if BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("Another update operation is still finishing. Try again shortly.".into());
    }
    let (send, receive) = mpsc::sync_channel(16);
    let cancel = Cancellation::default();
    let token = cancel.clone();
    let spawned = std::thread::Builder::new()
        .name("octosense-updater".into())
        .spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&token, &send)))
                    .unwrap_or_else(|_| {
                        Err("Update operation stopped unexpectedly. You can retry.".into())
                    });
            BUSY.store(false, Ordering::Release);
            if let Err(mpsc::SendError(WorkerEvent::Finished(result))) =
                send.send(WorkerEvent::Finished(Box::new(result)))
            {
                // The window closed just as a download finished. No view owns
                // this unused artifact, so do not leave it in private storage.
                if let Ok(Completed::Downloaded(artifact)) = *result {
                    let _ = artifact.discard();
                }
            }
            makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
        });
    if let Err(error) = spawned {
        BUSY.store(false, Ordering::Release);
        return Err(format!("Could not start update worker: {error}"));
    }
    Ok(Worker { receive, cancel })
}

#[derive(Script, ScriptHook, Widget)]
pub struct UpdaterView {
    #[deref]
    view: View,
    #[rust]
    initialized: bool,
    #[rust]
    channel: usize,
    #[rust]
    identity: Option<BuildIdentity>,
    #[rust]
    cache: Option<PathBuf>,
    #[rust]
    offer: Option<Offer>,
    #[rust]
    artifact: Option<VerifiedArtifact>,
    #[rust]
    task: Option<Worker>,
    #[rust]
    message: String,
    #[rust]
    android_request: u64,
    #[rust]
    android_standalone: bool,
    #[rust]
    android_can_install: bool,
    #[rust]
    android_install_pending: bool,
    #[rust]
    install_opened: bool,
}

impl UpdaterView {
    fn product() -> Product {
        if cfg!(target_os = "android") {
            Product::Home
        } else {
            Product::Desktop
        }
    }

    fn android_command(&mut self, cx: &mut Cx, operation: &str, mut value: serde_json::Value) {
        let id = REQUEST.fetch_add(1, Ordering::Relaxed);
        self.android_request = id;
        value["id"] = id.into();
        value["operation"] = operation.into();
        cx.android_integration("home_updater.command", &value.to_string());
    }

    fn initialize(&mut self, cx: &mut Cx) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let identity = BuildIdentity::from_env(Self::product());
        self.channel = match identity.default_channel() {
            Channel::Stable => 0,
            Channel::ReleaseCandidate => 1,
            Channel::Preview => 2,
        };
        self.view.label(cx, ids!(current)).set_text(
            cx,
            &format!(
                "Installed: {}",
                identity
                    .version()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "Development build (no release version)".into())
            ),
        );
        self.identity = Some(identity);
        self.cache = CACHE.get().cloned();
        self.message = "Check for an update when you are ready.".into();
        if cfg!(target_os = "android") {
            self.message = "Checking this Android installation…".into();
            self.android_command(cx, "update_info", serde_json::json!({}));
        } else if Platform::current(Self::product()).is_none() {
            self.message = "In-app updates are not available for this platform yet.".into();
        }
        self.refresh(cx);
    }

    fn refresh(&mut self, cx: &mut Cx) {
        let busy = self.task.is_some();
        let supported = Platform::current(Self::product()).is_some()
            && self.cache.is_some()
            && (!cfg!(target_os = "android") || self.android_standalone);
        self.view
            .label(cx, ids!(status))
            .set_text(cx, &self.message);
        self.view.button(cx, ids!(channel)).set_text(
            cx,
            match self.channel {
                0 => "Channel: Stable",
                1 => "Channel: Release candidate",
                _ => "Channel: Preview (all test releases)",
            },
        );
        self.view
            .button(cx, ids!(channel))
            .set_enabled(cx, !busy && !self.android_install_pending);
        self.view
            .button(cx, ids!(check))
            .set_enabled(cx, supported && !busy && !self.android_install_pending);
        self.view
            .button(cx, ids!(download))
            .set_visible(cx, self.offer.is_some() && self.artifact.is_none() && !busy);
        self.view.button(cx, ids!(cancel)).set_visible(cx, busy);
        self.view
            .button(cx, ids!(cancel))
            .set_enabled(cx, busy && !self.android_install_pending);
        self.view.button(cx, ids!(install)).set_visible(
            cx,
            self.artifact.is_some() && !busy && !self.android_install_pending,
        );
        self.view.button(cx, ids!(install)).set_text(
            cx,
            if cfg!(target_os = "android") {
                "Review and install update"
            } else {
                "Open verified installer"
            },
        );
        self.view.button(cx, ids!(permission)).set_visible(
            cx,
            cfg!(target_os = "android")
                && self.artifact.is_some()
                && !self.android_can_install
                && !busy
                && !self.android_install_pending,
        );
        self.view.button(cx, ids!(retry_install)).set_visible(
            cx,
            cfg!(target_os = "android")
                && (self.android_install_pending || self.artifact.is_some()),
        );
        self.view
            .button(cx, ids!(notes))
            .set_visible(cx, self.offer.is_some());
        let description = self
            .offer
            .as_ref()
            .map(|offer| {
                format!(
                    "OctoSense {}\n{} · {:.1} MB",
                    offer.version(),
                    offer.asset_name(),
                    offer.size() as f64 / 1_000_000.0
                )
            })
            .unwrap_or_default();
        self.view.label(cx, ids!(offer)).set_text(cx, &description);
        self.view.redraw(cx);
    }

    fn start(&mut self, task: Result<Worker, String>) {
        match task {
            Ok(task) => self.task = Some(task),
            Err(error) => self.message = error,
        }
    }

    fn retire_offer(&mut self) {
        if let Some(artifact) = self.artifact.take() {
            // A mounted DMG or installer may still need its source file.
            if !self.install_opened {
                let _ = artifact.discard();
            }
        }
        self.offer = None;
        self.install_opened = false;
    }

    fn check(&mut self) {
        if self.task.is_some() || self.android_install_pending {
            return;
        }
        let Some(cache) = self.cache.clone() else {
            return;
        };
        let Some(platform) = Platform::current(Self::product()) else {
            return;
        };
        if cfg!(target_os = "android") && !self.android_standalone {
            return;
        }
        let Some(identity) = self.identity.clone() else {
            return;
        };
        self.retire_offer();
        let channel = match self.channel {
            0 => Channel::Stable,
            1 => Channel::ReleaseCandidate,
            _ => Channel::Preview,
        };
        self.message = "Checking official releases…".into();
        self.start(worker(move |cancel, _| {
            let client = Client::new(cache).map_err(|e| e.to_string())?;
            client
                .check(&identity, channel, platform, cancel)
                .map(Completed::Checked)
                .map_err(|e| e.to_string())
        }));
    }

    fn download(&mut self) {
        if self.task.is_some() {
            return;
        }
        let (Some(cache), Some(offer)) = (self.cache.clone(), self.offer.clone()) else {
            return;
        };
        self.message = "Downloading update…".into();
        self.start(worker(move |cancel, send| {
            let client = Client::new(cache).map_err(|e| e.to_string())?;
            client
                .download(&offer, cancel, |progress| {
                    let _ = send.try_send(WorkerEvent::Progress(progress.received, progress.total));
                    makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
                })
                .map(Completed::Downloaded)
                .map_err(|e| e.to_string())
        }));
    }

    fn install(&mut self) {
        if self.task.is_some() || self.android_install_pending {
            return;
        }
        let Some(artifact) = self.artifact.clone() else {
            return;
        };
        // Android's PackageInstaller requires its own human confirmation as well.
        if cfg!(target_os = "android") && !makepad_widgets::makepad_platform::trusted_user_input() {
            self.message = "Tap Install directly on the phone to review this update.".into();
            return;
        }
        self.message = "Rechecking the downloaded file…".into();
        self.start(worker(move |cancel, _| {
            if cfg!(target_os = "android") {
                let path = artifact.revalidate().map_err(|e| e.to_string())?;
                if cancel.is_cancelled() {
                    return Err("Update cancelled.".into());
                }
                Ok(Completed::AndroidReady(path, artifact.sha256().to_owned()))
            } else {
                install::install(&artifact, cancel).map(Completed::Opened)
            }
        }));
        // Reserve the source while handoff is running: closing the view may
        // race with `open` successfully mounting it before its result arrives.
        // Retaining a verified cache file is safer than deleting a source the
        // operating system is already using.
        if self.task.is_some() {
            self.install_opened = true;
        }
    }

    fn poll(&mut self, cx: &mut Cx) {
        let events: Vec<_> = self
            .task
            .as_ref()
            .map(|task| task.receive.try_iter().collect())
            .unwrap_or_default();
        let changed = !events.is_empty();
        for event in events {
            match event {
                WorkerEvent::Progress(received, total) => {
                    self.message = format!(
                        "Downloading: {:.1} / {:.1} MB",
                        received as f64 / 1_000_000.0,
                        total as f64 / 1_000_000.0
                    )
                }
                WorkerEvent::Finished(result) => {
                    let result = *result;
                    let cancelled = self
                        .task
                        .as_ref()
                        .is_some_and(|task| task.cancel.is_cancelled());
                    self.task = None;
                    if cancelled && !matches!(&result, Ok(Completed::Opened(_))) {
                        if let Ok(Completed::Downloaded(artifact)) = result {
                            let _ = artifact.discard();
                        }
                        self.message = "Update cancelled; no installer was opened.".into();
                        self.install_opened = false;
                        continue;
                    }
                    match result {
                        Ok(Completed::Checked(result)) => {
                            self.message = match result.status {
                                CheckStatus::Available => "An update is available. Download it to continue.",
                                CheckStatus::Current => "You are up to date on this channel.",
                                CheckStatus::Development => "This is a development build. Install a release build to enable versioned updates.",
                                CheckStatus::Unavailable => "No compatible release is published on this channel yet.",
                            }.into();
                            self.offer = if result.status == CheckStatus::Available {
                                result.offer
                            } else {
                                None
                            };
                        }
                        Ok(Completed::Downloaded(artifact)) => {
                            self.artifact = Some(artifact);
                            self.message = if cfg!(target_os = "android") {
                                "Download verified. Save your work, then choose Review and install update. Android will ask you to confirm."
                            } else {
                                "Download verified. Save your work, then choose Open verified installer. Installation may restart OctoSense."
                            }.into();
                        }
                        Ok(Completed::Opened(message)) => {
                            self.message = message;
                            self.install_opened = true;
                        }
                        Ok(Completed::AndroidReady(path, digest)) => {
                            self.android_install_pending = true;
                            self.install_opened = true;
                            self.message =
                                "Waiting for Android to verify and confirm installation…".into();
                            self.android_command(
                                cx,
                                "update_install",
                                serde_json::json!({"path":path,"sha256":digest}),
                            );
                        }
                        Err(error) => {
                            self.install_opened = false;
                            self.message = error;
                        }
                    }
                }
            }
        }
        if changed {
            self.refresh(cx);
        }
        let packets = {
            let mut pending = ANDROID.lock().unwrap_or_else(|e| e.into_inner());
            let mut found = Vec::new();
            pending.retain(|value| {
                if value["id"].as_u64() == Some(self.android_request) {
                    found.push(value.clone());
                    false
                } else {
                    true
                }
            });
            found
        };
        for value in packets {
            if let Some(allowed) = value["can_install"].as_bool() {
                self.android_can_install = allowed;
            }
            let status = value["status"].as_str().unwrap_or("error");
            if value["operation"] == "update_info" && status == "ok" {
                self.cache = value["cache_dir"].as_str().map(PathBuf::from);
                self.android_standalone = value["standalone"].as_bool().unwrap_or(false);
                self.android_can_install = value["can_install"].as_bool().unwrap_or(false);
                self.view.label(cx, ids!(current)).set_text(
                    cx,
                    &format!(
                        "Installed: OctoSense Home {}",
                        value["version_name"].as_str().unwrap_or("unknown")
                    ),
                );
                self.message = if let Some(reason) =
                    value["message"].as_str().filter(|text| !text.is_empty())
                {
                    reason.chars().take(1024).collect()
                } else if self.android_standalone {
                    "Check for an update when you are ready.".into()
                } else {
                    "This installation uses a different signing identity. Use Settings → Updates for ROM updates, or obtain a compatible build from your distributor. Your installed Home and data will be kept.".into()
                };
                if self.android_standalone {
                    // An OS installation can outlive this window or replace the
                    // process. Recover its durable status before offering work.
                    self.android_install_pending = true;
                    self.android_command(cx, "update_status", serde_json::json!({}));
                }
            } else {
                self.message = value["message"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(if status == "idle" {
                        "Check for an update when you are ready."
                    } else {
                        status
                    })
                    .chars()
                    .take(1024)
                    .collect();
                self.android_install_pending =
                    matches!(status, "preparing" | "awaiting_confirmation");
                if status == "permission_required" {
                    self.android_can_install = false;
                }
            }
            self.refresh(cx);
        }
    }
}

impl Widget for UpdaterView {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.initialize(cx);
        self.poll(cx);
        self.view.draw_walk(cx, scope, walk)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.initialize(cx);
        self.poll(cx);
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            let changed = [
                ids!(channel),
                ids!(check),
                ids!(download),
                ids!(cancel),
                ids!(install),
                ids!(notes),
                ids!(permission),
                ids!(retry_install),
            ]
            .iter()
            .any(|id| self.view.button(cx, *id).clicked(actions));
            if !changed {
                return;
            }
            if self.view.button(cx, ids!(channel)).clicked(actions)
                && self.task.is_none()
                && !self.android_install_pending
            {
                self.channel = (self.channel + 1) % 3;
                self.retire_offer();
                self.message = "Channel changed. Check for updates to continue.".into();
            }
            if self.view.button(cx, ids!(check)).clicked(actions) {
                self.check();
            }
            if self.view.button(cx, ids!(download)).clicked(actions) {
                self.download();
            }
            if self.view.button(cx, ids!(cancel)).clicked(actions) {
                if let Some(task) = &self.task {
                    task.cancel.cancel();
                    self.message = "Cancelling…".into();
                }
            }
            if self.view.button(cx, ids!(install)).clicked(actions) {
                self.install();
            }
            if self.view.button(cx, ids!(notes)).clicked(actions) {
                if let Some(offer) = &self.offer {
                    cx.open_url(&offer.release_url(), OpenUrlInPlace::No);
                }
            }
            if self.view.button(cx, ids!(permission)).clicked(actions)
                && makepad_widgets::makepad_platform::trusted_user_input()
            {
                self.android_command(cx, "update_allow_install", serde_json::json!({}));
            }
            if self.view.button(cx, ids!(retry_install)).clicked(actions) {
                self.android_command(cx, "update_status", serde_json::json!({}));
            }
            self.refresh(cx);
        }
    }
}

pub struct UpdaterModule;
pub static UPDATER_MODULE: UpdaterModule = UpdaterModule;
impl AppModule for UpdaterModule {
    fn id(&self) -> &'static str {
        "updater"
    }
    fn label(&self) -> &'static str {
        "OctoSense Updates"
    }
    fn register(&self, vm: &mut ScriptVm) {
        script_mod(vm);
    }
    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &[]
    }
    fn create(&self, vm: &mut ScriptVm, _: ValidatedOpen, _: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, { use mod.widgets.* UpdaterView {} });
        let root = WidgetRef::script_from_value(vm, value);
        let closing = root.clone();
        InstanceParts {
            root,
            executor: Box::new(NoTools),
            shutdown: Box::new(move |vm| {
                if let Some(mut view) = closing.borrow_mut::<UpdaterView>() {
                    if let Some(task) = view.task.take() {
                        task.cancel.cancel();
                    }
                    view.retire_offer();
                    if cfg!(target_os = "android") {
                        // Cancel queued Settings openings as well as APK prep.
                        // A session already handed to Android is preserved.
                        view.android_command(vm.cx_mut(), "update_cancel", serde_json::json!({}));
                    }
                }
            }),
        }
    }
}
struct NoTools;
impl ServiceExecutor for NoTools {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new(
            "updater",
            "OctoSense Updates",
            "Install reviewed OctoSense releases. No agent tools.",
        )
    }
    fn execute(&mut self, _: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(
            &call.call_id,
            "Updates can only be requested in the host-owned Updates window",
        ))
    }
}
