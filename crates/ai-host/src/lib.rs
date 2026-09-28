//! The OctoSense shell's AI services, one entry point.
//!
//! A shell calls:
//!
//! ```ignore
//! // handle_startup, before anything connects to the kernel or opens AI providers:
//! octosense_ai_host::start(octosense_ai_host::Host::platform(cx.get_data_dir()));
//! // every event, early (opens the QR scanner / picker the service asked for):
//! octosense_ai_host::handle_event(cx, event);
//! // desktop, where drags and drops are routed (`app_at`: the app whose window is at a point):
//! if octosense_ai_host::handle_drop(event, &app_at) { return; }
//! // Android extension packet `qr.image.result`:
//! octosense_ai_host::qr_image_result(id, &status, &detail);
//! // module host, around `module.create`:
//! let offer = octosense_ai_host::offer(module, &scope);
//! let parts = module.create(vm, open, handles);
//! let assistant = offer.finish();            // keep it with the instance; dropping it releases
//! // Event::Shutdown:
//! octosense_ai_host::shutdown();
//! ```
//!
//! What it runs:
//!
//! - **The octos kernel** (`cfg(kernel)`: feature `octos-core`, or any native
//!   mobile target), a shell service: configured once by [`start`] with the
//!   app's data dir (a phone's core dir is `<data dir>/octos-home/.octos`),
//!   started when a consumer (AppCard, Rinx) first connects, restarted by
//!   the `llm` service after a provider change, stopped by [`shutdown`].
//!   How it runs is the [`KernelSource`].
//! - **The `llm` host service** (feature `llm`) the AI providers system app
//!   (`os.ai-providers`) calls, writing the kernel's profile under
//!   [`core_dir`], with the platform's [`QrImport`].
//! - **The `model` host service** (feature `llm`): contained apps granted
//!   `model` make one-shot, schema-checked calls to the person's providers
//!   within a per-app budget (`octosense_llm_service::complete`).
//! - **Apps' assistant access** (Rinx ADR 0007): when the shell creates a
//!   native module instance whose declared `octos.*` services the host
//!   [`Policy`] grants, [`offer`] makes ONE octos peer for that app (owned by
//!   the shell's system agent, on the shell's kernel) and offers the instance
//!   a scoped service for the duration of its `create` only. A module that
//!   declares or is granted nothing gets nothing, and no peer is allocated.
//!   Dropping the [`Assistant`] releases the instance's leases and
//!   interrupts its peer's running work; the kernel and other apps go on.
//! - **The `octos` host service** ([`contained`]): contained apps the Card
//!   runner hosts that declare `octos.*` get their own peer (`card.<app id>`)
//!   under the same contract, while [`Policy::contained_apps`] is on. Tool
//!   approvals are declined until the Card runner has an approval sheet.

mod bridge;
pub mod contained;
mod qr;

pub use bridge::{Bridge, Done};
pub use qr::{ImageSource, PickError, QrImport, DROP_APP};

/// The app-peers contract (for a shell's module-host tests: `Deployment`,
/// `SettingsEntry`, `injection`).
pub use octosense_app_peers as app_peers;
/// The kernel service itself, where this build hosts one.
#[cfg(kernel)]
pub use octosense_kernel as kernel;

use makepad_app_module::{AppModule, InstanceScope};
use makepad_widgets::*;
use std::path::PathBuf;
use std::sync::OnceLock;

/// How this shell runs the octos kernel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KernelSource {
    /// Android: the APK's `liboctos.so` (`tools/kernel-artifact.py` bundles it).
    Bundled,
    /// OpenHarmony: the canonical core linked in-process.
    InProcess,
    /// Desktop: the binary `$OCTOS_APP_CORE_BIN` names; none without it (a
    /// developer's own `octos serve` is never touched).
    Env,
    /// Desktop or Android: this binary.
    Program(PathBuf),
    /// No kernel: nothing starts, no app gets an assistant. The providers are
    /// still saved.
    None,
}

impl KernelSource {
    /// This platform's kernel: bundled on Android, in-process on
    /// OpenHarmony, `$OCTOS_APP_CORE_BIN` on a desktop, none on iOS.
    pub fn platform() -> Self {
        if cfg!(target_os = "android") {
            KernelSource::Bundled
        } else if cfg!(target_env = "ohos") {
            KernelSource::InProcess
        } else if cfg!(target_os = "ios") {
            KernelSource::None
        } else {
            KernelSource::Env
        }
    }

    /// Why this source cannot run on this target, if it cannot.
    #[cfg_attr(not(kernel), allow(dead_code))]
    fn unsupported_here(&self) -> Option<&'static str> {
        let android = cfg!(target_os = "android");
        let ohos = cfg!(target_env = "ohos");
        let ios = cfg!(target_os = "ios");
        let desktop = !(android || ohos || ios);
        match self {
            KernelSource::Bundled if !android => Some("a bundled kernel is Android's"),
            KernelSource::InProcess if !ohos => Some("an in-process kernel is OpenHarmony's"),
            KernelSource::Env if !desktop => Some("$OCTOS_APP_CORE_BIN is a desktop's"),
            KernelSource::Program(_) if !(desktop || android) => Some("this platform cannot exec a kernel"),
            _ => None,
        }
    }
}

/// Which native modules may use the assistant, and with which `octos.*`
/// services (exact names; others are ignored).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    grants: Vec<(String, Vec<String>)>,
    /// Whether contained apps (the Card runner's) get the `octos` service.
    contained_apps: bool,
}

impl Policy {
    /// Nobody gets the assistant.
    pub fn none() -> Self {
        Policy::default()
    }

    /// The native modules that ship with OctoSense: Rinx (its native mini-app
    /// host serves these to reviewed mini apps). The person's AI provider
    /// choice lives in AI providers; a per-app toggle is future work.
    pub fn shipped() -> Self {
        Policy::none().allow("rinx", octosense_app_peers::OCTOS_SERVICES).with_contained_apps(true)
    }

    /// Turn the `octos` service for contained apps on or off (the shell's
    /// switch; an app still gets only the services its manifest declares).
    pub fn with_contained_apps(mut self, on: bool) -> Self {
        self.contained_apps = on;
        self
    }

    /// Whether contained apps get the `octos` service.
    pub fn contained_apps(&self) -> bool {
        self.contained_apps
    }

    /// Also grant `module` these services.
    pub fn allow<'a>(mut self, module: &str, services: impl IntoIterator<Item = &'a str>) -> Self {
        self.grants.push((module.to_owned(), services.into_iter().map(str::to_owned).collect()));
        self
    }

    pub fn grants(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.grants.iter().map(|(m, s)| (m.as_str(), s.as_slice()))
    }
}

/// What a shell tells [`start`].
#[derive(Clone, Debug)]
pub struct Host {
    /// The app's data dir (`cx.get_data_dir()`): a phone's kernel home lives
    /// under it.
    pub data_dir: Option<String>,
    pub kernel: KernelSource,
    pub qr_import: QrImport,
    pub policy: Policy,
}

impl Host {
    /// This platform's defaults with the shipped policy.
    pub fn platform(data_dir: Option<String>) -> Self {
        Host { data_dir, kernel: KernelSource::platform(), qr_import: QrImport::platform(), policy: Policy::shipped() }
    }
}

/// What [`start`] set up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Started {
    /// Where the kernel's profile lives (`<core_dir>/profiles/_main.json`).
    pub core_dir: Option<PathBuf>,
    /// Whether a kernel can start here (on first use); why not otherwise.
    pub kernel: Result<(), String>,
    /// Whether the `llm` service is registered.
    pub llm: bool,
}

struct State {
    qr_import: QrImport,
    #[cfg_attr(not(kernel), allow(dead_code))]
    kernel: bool,
    started: Started,
}

static STATE: OnceLock<State> = OnceLock::new();

/// Set up the shell's AI services, once (later calls return the first
/// result): the kernel's configuration (nothing starts until a consumer
/// connects), the host policy, and the `llm` service with the QR import.
pub fn start(host: Host) -> &'static Started {
    let state = STATE.get_or_init(|| {
        let (kernel, kernel_status) = configure_kernel(&host);
        grant_policy(&host.policy, kernel);
        let core_dir = core_dir(host.data_dir.clone());
        let llm = register_llm(core_dir.clone(), host.qr_import);
        register_contained(kernel, &host.policy);
        State { qr_import: host.qr_import, kernel, started: Started { core_dir, kernel: kernel_status, llm } }
    });
    &state.started
}

/// Whether [`start`] ran.
pub fn is_started() -> bool {
    STATE.get().is_some()
}

#[cfg(kernel)]
fn configure_kernel(host: &Host) -> (bool, Result<(), String>) {
    if host.kernel == KernelSource::None {
        log!("octos: no kernel on this shell; the providers are still saved");
        return (false, Err("no kernel on this shell".into()));
    }
    if let Some(why) = host.kernel.unsupported_here() {
        log!("octos: {why}; no kernel on this shell");
        return (false, Err(why.into()));
    }
    // The kernel's stderr and the core's starts and stops go to the shell's
    // log (logcat on Android), not the `log` facade no logger listens to.
    let mut options = octosense_kernel::Options::default().log(|line| log!("{line}"));
    if let Some(dir) = host.data_dir.clone().filter(|d| !d.is_empty()) {
        options = options.app_data_dir(dir);
    }
    if let KernelSource::Program(program) = &host.kernel {
        options = options.program(program.clone());
    }
    octosense_kernel::configure(options);
    match octosense_kernel::launch() {
        Ok(_) => {
            log!("octos: kernel service ready (starts on first use), core dir {:?}", octosense_kernel::core_dir());
            (true, Ok(()))
        }
        Err(why) => {
            log!("octos: {why}; the providers are still saved under {:?}", octosense_kernel::core_dir());
            // Configured all the same: a kernel that appears later (e.g. a
            // binary installed while running) is picked up on first use.
            (true, Err(why.to_string()))
        }
    }
}

#[cfg(not(kernel))]
fn configure_kernel(_host: &Host) -> (bool, Result<(), String>) {
    (false, Err("this build has no kernel (feature `octos-core`)".into()))
}

/// The kernel's octos home, where the `llm` service writes: the kernel's core
/// dir (`$OCTOS_APP_CORE_DIR`, else on a phone `<data dir>/octos-home/.octos`,
/// else `$HOME/octos-home/.octos`).
pub fn core_dir(data_dir: Option<String>) -> Option<PathBuf> {
    #[cfg(kernel)]
    {
        let _ = &data_dir;
        if let Some(dir) = octosense_kernel::core_dir() {
            return Some(dir);
        }
    }
    // Without the kernel service (a desktop build without `octos-core`): the
    // same rule, for the profile alone.
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

/// The `octos` service for contained apps, where this shell hosts a kernel.
/// Registered even with the switch off, so an app hears why it gets nothing.
fn register_contained(kernel: bool, policy: &Policy) {
    #[cfg(kernel)]
    if kernel {
        let on = policy.contained_apps();
        octosense_appstore::services::register_host_service(Box::new(contained::ContainedOctos::new(
            on,
            std::sync::Arc::new(contained::KernelPeers),
        )));
        log!("octos: contained apps' service registered ({})", if on { "on" } else { "off" });
        return;
    }
    let _ = (kernel, policy);
}

#[cfg(feature = "llm")]
fn register_llm(core_dir: Option<PathBuf>, import: QrImport) -> bool {
    let mut options = octosense_llm_service::Options::default();
    match &core_dir {
        Some(dir) => {
            log!("llm: provider profile under {}", dir.display());
            options = options.core_dir(dir.clone());
        }
        None => log!("llm: no core dir; the service uses its own default"),
    }
    if import.camera {
        options = options.scanner(std::sync::Arc::new(qr::CameraScanner));
    }
    if import.image != ImageSource::None {
        options = options.image_picker(std::sync::Arc::new(qr::ImagePicker));
    }
    if import.drops {
        options = options.image_drops(true);
    }
    // With `octos-core` the service itself restarts the kernel after a
    // change; its consumers reconnect.
    octosense_llm_service::register_with(options.clone());
    // `model` (ADR 0002, `model.complete`): contained apps' one-shot model
    // calls over the same providers, with per-app budgets. Apps granted the
    // `model` capability only; the ledger lives in the Card runner's host
    // dir, attached at the first call.
    octosense_llm_service::register_model(&options, octosense_llm_service::complete::Options::default());
    log!("model: service registered (one-shot calls; granted apps only)");
    true
}

#[cfg(not(feature = "llm"))]
fn register_llm(_core_dir: Option<PathBuf>, _import: QrImport) -> bool {
    false
}

/// Every event, on the UI thread, early: opens the scanner or picker the
/// `llm` service asked for and completes it from the platform's answer.
pub fn handle_event(cx: &mut Cx, event: &Event) {
    if let Some(state) = STATE.get() {
        qr::pump(cx, event, state.qr_import);
    }
}

/// Desktop: a drag or drop of one image file over the AI providers window
/// while its import sheet waits for one (`app_at`: the short id of the app
/// whose window is at a point). True when the event was the service's.
pub fn handle_drop(event: &Event, app_at: &dyn Fn(Vec2d) -> Option<String>) -> bool {
    if !matches!(event, Event::Drag(_) | Event::Drop(_)) || !STATE.get().is_some_and(|s| s.qr_import.drops) {
        return false;
    }
    #[cfg(feature = "llm")]
    return qr::drop_event(event, app_at);
    #[cfg(not(feature = "llm"))]
    {
        let _ = app_at;
        false
    }
}

/// Android: the extension's `qr.image.result` packet for AI providers'
/// "Choose image".
pub fn qr_image_result(id: u64, status: &str, detail: &str) {
    qr::image_result(id, status, detail)
}

/// `Event::Shutdown`: stop the kernel, if one runs, and let it release its
/// data dir (5 s at most).
pub fn shutdown() {
    #[cfg(kernel)]
    octosense_kernel::shutdown();
}

/// Whether a kernel is running now (it starts on first use).
pub fn kernel_running() -> bool {
    #[cfg(kernel)]
    return octosense_kernel::status().running;
    #[cfg(not(kernel))]
    false
}

// ---- apps' assistant access -------------------------------------------------

/// The host policy in force: [`Policy::shipped`] until [`start`] installs
/// the shell's (a module host's own tests create modules without `start`).
#[cfg(kernel)]
fn host_policy() -> &'static octosense_app_peers::hosted::HostPolicy {
    static POLICY: OnceLock<octosense_app_peers::hosted::HostPolicy> = OnceLock::new();
    POLICY.get_or_init(|| {
        let policy = octosense_app_peers::hosted::HostPolicy::default();
        for (module, services) in Policy::shipped().grants() {
            policy.allow(module, services.iter().map(String::as_str));
        }
        policy
    })
}

fn grant_policy(policy: &Policy, kernel: bool) {
    #[cfg(kernel)]
    {
        let shipped = Policy::shipped();
        for (module, _) in shipped.grants() {
            if !kernel || !policy.grants().any(|(m, _)| m == module) {
                host_policy().deny(module);
            }
        }
        if kernel {
            for (module, services) in policy.grants() {
                host_policy().allow(module, services.iter().map(String::as_str));
            }
        }
    }
    #[cfg(not(kernel))]
    let _ = (policy, kernel);
}

/// Grant `module` these services now (tests, and a future per-app toggle).
pub fn grant<'a>(module: &str, services: impl IntoIterator<Item = &'a str>) {
    #[cfg(kernel)]
    host_policy().allow(module, services);
    #[cfg(not(kernel))]
    let _ = (module, services.into_iter().count());
}

/// One instance's assistant service, held by the module host with the
/// instance. Dropping it releases the instance's leases and contexts.
pub struct Assistant {
    #[cfg(kernel)]
    broker: octosense_app_peers::broker::Broker,
}

impl Assistant {
    /// The instance is going away: release its leases and contexts (the same
    /// as dropping it).
    pub fn release(self) {}
}

impl Drop for Assistant {
    fn drop(&mut self) {
        #[cfg(kernel)]
        octosense_app_peers::OctosAppService::release(&self.broker);
    }
}

impl std::fmt::Debug for Assistant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Assistant")
    }
}

/// The service offered to one instance while it is created. [`Offer::finish`]
/// right after `create` withdraws what the module did not take; so does
/// dropping it (a `create` that failed).
#[must_use = "finish the offer after `create`"]
pub struct Offer {
    module: &'static str,
    scope: String,
    assistant: Option<Assistant>,
}

/// Before `module.create`: offer the instance its service, if it gets one —
/// the kernel is hosted, [`start`] ran, and the policy grants some of the
/// `octos.*` services the module declares.
pub fn offer(module: &dyn AppModule, scope: &InstanceScope) -> Offer {
    let scope = scope.to_string();
    #[cfg(kernel)]
    let assistant = STATE.get().is_none_or(|s| s.kernel).then(|| {
        // Before `start` (a module host's own tests) the kernel keeps its
        // defaults; after it, only a shell that hosts a kernel offers.
        let broker = octosense_app_peers::hosted::launch(
            module.id(),
            module.label(),
            module.capabilities().iter().copied(),
            host_policy(),
        )?;
        octosense_app_peers::hosted::offer(module.id(), &scope, &broker);
        Some(Assistant { broker })
    }).flatten();
    #[cfg(not(kernel))]
    let assistant = None;
    Offer { module: module.id(), scope, assistant }
}

impl Offer {
    /// Whether this instance is offered a service.
    pub fn is_offered(&self) -> bool {
        self.assistant.is_some()
    }

    /// After `module.create`: drop an offer the module did not take; the
    /// instance's assistant, if it has one.
    pub fn finish(mut self) -> Option<Assistant> {
        if self.assistant.is_some() {
            let taken = !self.withdraw();
            log!(
                "ai-host: {} ({}) {} its assistant service",
                self.module,
                self.scope,
                if taken { "took" } else { "did not take" }
            );
        }
        self.assistant.take()
    }

    /// Drop the offer if it is still there; whether it was (not taken).
    fn withdraw(&self) -> bool {
        self.assistant.is_some() && octosense_app_peers::injection::withdraw(self.module, &self.scope)
    }
}

impl Drop for Offer {
    fn drop(&mut self) {
        let _ = self.withdraw();
    }
}

#[cfg(test)]
mod tests;
