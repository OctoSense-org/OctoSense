//! The `llm` host service: the assistant's LLM providers, keys kept by the
//! host. (Skeleton: the registration API is final; methods follow.)
use octosense_appstore::services::{HostService, Replier, ServiceCall, ServiceHost};
use std::path::PathBuf;
use std::sync::Arc;

pub mod vault;

use vault::Vault;

/// Called with a scan's result, once, from any thread: the decoded text, or
/// why there is none (`"cancelled"`, `"permission_denied"`, `"unsupported"`…).
pub type ScanDone = Box<dyn FnOnce(Result<String, String>) + Send>;

/// The device's QR scanner, provided by the shell (on Android, Makepad's
/// `cx.show_qr_scanner()` and its `NativeQrScanned` / `NativeQrCancelled`
/// actions). The service never links Makepad itself.
pub trait QrScanner: Send + Sync {
    /// Open the scanner over everything and call `done` exactly once.
    fn scan(&self, done: ScanDone);
}

/// Called after the provider set changed on disk (a save, a removal, an
/// import): the shell restarts the AppCard kernel so it reads the new
/// profile. Runs on whichever thread made the change.
pub type OnChanged = Arc<dyn Fn() + Send + Sync>;

/// What a shell hands the service. `Options::default()` is what
/// [`register`] uses.
#[derive(Clone, Default)]
pub struct Options {
    /// The kernel's octos home, holding `profiles/_main.json`. `None`:
    /// `octosense_llm_config::profile::default_core_dir()`.
    pub core_dir: Option<PathBuf>,
    /// Where keys go. `None`: [`vault::platform`] for the core dir.
    pub vault: Option<Arc<dyn Vault>>,
    /// The camera scanner, where the device has one: the import sheet then
    /// offers "Scan", and the app shows "Scan QR from desktop".
    pub scanner: Option<Arc<dyn QrScanner>>,
    pub on_changed: Option<OnChanged>,
}

impl Options {
    pub fn core_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.core_dir = Some(dir.into());
        self
    }
    pub fn vault(mut self, vault: Arc<dyn Vault>) -> Self {
        self.vault = Some(vault);
        self
    }
    pub fn scanner(mut self, scanner: Arc<dyn QrScanner>) -> Self {
        self.scanner = Some(scanner);
        self
    }
    pub fn on_changed(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_changed = Some(Arc::new(f));
        self
    }
}

/// Offer the service to the Card runner with the default core dir and the
/// platform's vault, no scanner and no change hook.
pub fn register() {
    register_with(Options::default());
}

/// Offer the service as `options` say. A second call replaces the first.
pub fn register_with(options: Options) {
    let core_dir = options
        .core_dir
        .clone()
        .or_else(octosense_llm_config::profile::default_core_dir)
        .unwrap_or_else(|| std::env::temp_dir().join("octos-home/.octos"));
    let vault = options.vault.clone().unwrap_or_else(|| vault::platform(&core_dir));
    octosense_appstore::services::register_host_service(Box::new(LlmService { core_dir, vault, options }));
}

pub struct LlmService {
    #[allow(dead_code)]
    core_dir: PathBuf,
    #[allow(dead_code)]
    vault: Arc<dyn Vault>,
    #[allow(dead_code)]
    options: Options,
}

impl HostService for LlmService {
    fn family(&self) -> &'static str {
        "llm"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(Err(format!("llm.{} is not implemented yet", call.method())));
    }
}
