//! The shells' former `app_peers_host` and `llm_host` tests, against the
//! crate's entry points. (The module-host tests that create real instances,
//! Rinx included, stay with the shell's module host: they exercise it.)

use super::*;
use makepad_app_module::*;

/// A module that declares `caps` and, inside a create, claims its offer the
/// way a hosted module does.
struct Probe(&'static str, &'static [&'static str]);

impl AppModule for Probe {
    fn id(&self) -> &'static str {
        self.0
    }
    fn label(&self) -> &'static str {
        "Probe"
    }
    fn capabilities(&self) -> &'static [&'static str] {
        self.1
    }
    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }
    fn register(&self, _vm: &mut ScriptVm) {}
    fn create(&self, _vm: &mut ScriptVm, _open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        unreachable!("the tests stand in for create with `claim`")
    }
}

/// What a module's create does: take the service offered to this instance.
fn claim(module: &Probe, scope: &InstanceScope) -> bool {
    octosense_app_peers::injection::claim(module.0, &scope.to_string()).is_some()
}

static AI_PROBE: Probe = Probe("assistant-probe", &["storage", "octos.session.open", "octos.turn.start"]);
static PLAIN_PROBE: Probe = Probe("plain-probe", &["storage", "net"]);
static UNGRANTED_PROBE: Probe = Probe("ungranted-probe", &["octos.turn.start"]);

/// Rinx ADR 0007: a granted module is offered its service at creation and
/// only then; a module that declares nothing, or is not granted what it
/// declares, gets none and no peer.
#[cfg(kernel)]
#[test]
fn a_granted_module_gets_its_service_at_creation_and_others_get_none() {
    grant("assistant-probe", ["octos.session.open", "octos.turn.start"]);

    let scope = InstanceScope::new(1, 1);
    let offered = offer(&AI_PROBE, &scope);
    assert!(offered.is_offered());
    assert!(claim(&AI_PROBE, &scope), "the granted module got a scoped service");
    let assistant = offered.finish();
    assert!(assistant.is_some());

    let scope = InstanceScope::new(2, 2);
    let offered = offer(&PLAIN_PROBE, &scope);
    assert!(!claim(&PLAIN_PROBE, &scope), "a module without assistant services gets none");
    assert!(offered.finish().is_none(), "no peer is allocated for it");

    let scope = InstanceScope::new(3, 3);
    let offered = offer(&UNGRANTED_PROBE, &scope);
    assert!(!claim(&UNGRANTED_PROBE, &scope), "declaring is not being granted");
    assert!(offered.finish().is_none());

    // An offer never outlives its create: nothing is left to claim, whether
    // the module took it or not, or the create failed.
    let scope = InstanceScope::new(4, 4);
    let untaken = offer(&AI_PROBE, &scope).finish();
    assert!(untaken.is_some(), "the instance keeps its assistant even if create did not claim");
    assert!(!claim(&AI_PROBE, &scope));
    let scope = InstanceScope::new(5, 5);
    drop(offer(&AI_PROBE, &scope));
    assert!(!claim(&AI_PROBE, &scope), "a failed create withdraws the offer");

    // Releasing (dropping) the assistant leaves the kernel alone.
    drop(assistant);
    drop(untaken);
    assert!(!kernel_running(), "offering and releasing start no kernel");
}

/// Without the kernel (a desktop build without `octos-core`) no module is
/// ever offered a service.
#[cfg(not(kernel))]
#[test]
fn without_a_kernel_nobody_gets_an_assistant() {
    grant("assistant-probe", ["octos.session.open", "octos.turn.start"]);
    let scope = InstanceScope::new(1, 1);
    let offered = offer(&AI_PROBE, &scope);
    assert!(!offered.is_offered());
    assert!(!claim(&AI_PROBE, &scope));
    assert!(offered.finish().is_none());
    let _ = (&PLAIN_PROBE, &UNGRANTED_PROBE);
}

/// Before `start`, the shipped policy is in force: Rinx is offered its
/// service (a module host's own tests create it without `start`).
#[cfg(kernel)]
#[test]
fn rinx_is_granted_before_start() {
    static RINX_PROBE: Probe = Probe("rinx", &["octos.session.open", "octos.turn.start", "net"]);
    let scope = InstanceScope::new(9, 9);
    let offered = offer(&RINX_PROBE, &scope);
    assert!(offered.is_offered());
    assert!(offered.finish().is_some());
}

/// The shipped policy grants Rinx exactly the assistant services.
#[test]
fn the_shipped_policy_grants_rinx_the_assistant() {
    let policy = Policy::shipped();
    let grants: Vec<_> = policy.grants().collect();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].0, "rinx");
    assert_eq!(grants[0].1, octosense_app_peers::OCTOS_SERVICES.map(String::from));
    assert!(Policy::none().grants().next().is_none());
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

#[test]
fn each_platform_runs_its_own_kernel() {
    let source = KernelSource::platform();
    assert_eq!(source.unsupported_here(), None, "the platform default always runs here");
    assert_eq!(KernelSource::None.unsupported_here(), None);
    if cfg!(not(any(target_os = "android", target_os = "ios", target_env = "ohos"))) {
        assert_eq!(source, KernelSource::Env);
        assert!(KernelSource::Bundled.unsupported_here().is_some());
        assert!(KernelSource::InProcess.unsupported_here().is_some());
        assert_eq!(KernelSource::Program("/bin/octos".into()).unsupported_here(), None);
    }
}

/// `start` configures, registers and grants, once; no kernel starts until a
/// consumer connects (ADR 0007 criterion 7). One test owns the process-wide
/// start so the others see a fresh crate.
#[test]
fn start_is_once_and_starts_no_kernel() {
    let dir = std::env::temp_dir().join(format!("octosense-ai-host-{}", std::process::id()));
    let host = Host {
        data_dir: Some(dir.to_string_lossy().into_owned()),
        kernel: KernelSource::platform(),
        qr_import: QrImport::paste_only(),
        policy: Policy::shipped(),
    };
    let started = start(host.clone());
    assert!(is_started());
    assert_eq!(started.llm, cfg!(feature = "llm"));
    assert_eq!(started.core_dir, core_dir(host.data_dir.clone()));
    if cfg!(not(kernel)) {
        assert!(started.kernel.is_err());
    }
    assert!(!kernel_running(), "start starts no kernel");
    let again = start(Host { qr_import: QrImport::platform(), ..host });
    assert!(std::ptr::eq(started, again), "start runs once");
    shutdown();
    assert!(!kernel_running());
}
