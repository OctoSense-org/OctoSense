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

/// The shipped policy grants each native app the agent services its
/// `native-apps.json` entry declares (the generated `native_agents`), not
/// code: Rinx exactly the assistant services.
#[test]
fn the_shipped_policy_grants_each_native_app_its_declared_services() {
    let policy = Policy::shipped();
    let grants: Vec<_> = policy.grants().collect();
    let rinx = grants.iter().find(|(app, _)| *app == "rinx").expect("Rinx is granted");
    assert_eq!(rinx.1, octosense_app_peers::OCTOS_SERVICES.map(String::from));
    assert!(Policy::none().grants().next().is_none());
    let generated: Vec<(&str, Vec<String>)> = crate::native_agents::NATIVE_AGENTS.iter().map(|(app, s)| (*app, s.iter().map(|s| s.to_string()).collect())).collect();
    let shipped: Vec<(&str, Vec<String>)> = policy.grants().map(|(app, s)| (app, s.to_vec())).collect();
    assert_eq!(shipped, generated, "Policy::shipped() is the manifest's agent block");
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../native-apps.json")).unwrap();
    for (app, services) in &grants {
        let entry = manifest["apps"].as_array().unwrap().iter().find(|a| a["id"] == *app).unwrap();
        let declared: Vec<String> = entry["agent"]["octos"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
        assert_eq!(services.to_vec(), declared, "{app}");
    }
}

#[test]
fn the_core_dir_is_octosenses_own_octos_home_under_its_data_dir() {
    if std::env::var_os("OCTOS_APP_CORE_DIR").is_some() {
        return;
    }
    let dir = core_dir(Some("/data/user/0/app/files".into()));
    // With the kernel service, the kernel's (configured) core dir.
    #[cfg(kernel)]
    assert_eq!(dir, octosense_kernel::core_dir());
    // Without it, every platform: `<data dir>/octos-home/.octos`, never
    // `~/octos-home`.
    #[cfg(not(kernel))]
    assert_eq!(dir, Some(PathBuf::from("/data/user/0/app/files/octos-home/.octos")));
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
    // `start` registers the kernel's `octos` service and peer factory for the
    // whole process; without the contained tests' lock it can replace theirs
    // mid-test, and they then fail with "no octos kernel".
    let _contained = crate::contained::tests::serial();
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

/// A host caption must have a width bound: Fit-width text cannot wrap even
/// when the Label prototype's native flow permits wrapping.
#[cfg(feature = "llm")]
#[test]
fn provider_import_caption_wraps_inside_a_phone_sheet() {
    use makepad_widgets::makepad_draw::cx_draw::CxDraw;
    let sheet = octosense_llm_service::sheets::import(false, true, false);
    let caption = sheet.split_once("let Caption = ").unwrap().1.split_once('\n').unwrap().0;
    let text = "Choose image reads the code from a screenshot or photo.";
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let root = cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        desktop_style::install(vm, desktop_style::StyleSheet::load(desktop_style::DesktopStyle::Android));
        vm.with_reload(makepad_widgets::script_mod);
        vm.bx.captured_errors = Some(Vec::new());
        let value = vm.eval(ScriptMod {code:format!("use mod.prelude.widgets.*\nlet Caption = {caption}\nView{{width: Fill height: Fit padding: 16 flow: Down caption := Caption{{text: \"{text}\"}}}}"),..Default::default()});
        let root = WidgetRef::script_from_value(vm,value);
        assert!(vm.take_errors().is_empty());
        root
    });
    let pass = DrawPass::new(&mut cx); let mut list = DrawList2d::new(&mut cx);
    for width in [280.,340.,430.] {
        let size = dvec2(width,180.); pass.set_size(&mut cx,size);
        let event = DrawEvent::default();
        {
            let mut draw = CxDraw::new(&mut cx,&event); let mut draw = Cx2d::new(&mut draw);
            draw.begin_pass(&pass,Some(1.)); list.begin_always(&mut draw); draw.begin_root_turtle(size,Layout::default());
            root.draw_walk_all(&mut draw,&mut Scope::empty(),Walk::fixed(size.x,size.y));
            draw.end_turtle(); list.end(&mut draw); draw.end_pass(&pass);
        }
        let caption = root.label(&cx,ids!(caption));
        assert_eq!(caption.text(),text);
        let area = caption.area().rect(&cx);
        let text_rect = caption.borrow().unwrap().text_layout_rect;
        assert!(area.size.x <= width-32.+0.1 && area.pos.x>=16.);
        assert!(text_rect.size.x <= area.size.x+0.5,"whole instruction fits at {width}: {text_rect:?}/{area:?}");
        if width==280. { assert!(area.size.y>20.,"narrow caption takes multiple lines"); }
    }
}
