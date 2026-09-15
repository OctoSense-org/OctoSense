//! Octoscript-AppCard inside OctoSense, Phase B: the WHOLE app in a home
//! tile, in-process, in an isolate of its own.
//!
//! Phase A rendered one pre-lowered card here. Now the module hosts
//! `octos-app` itself — the crate the standalone AppCard APK is built from,
//! consumed as a library: its routing brain (AMA prompt, `route_to_app`,
//! the per-domain app agents and the L0 corpus), the card store / transport
//! crates and the `OctosUiAgent` with its tokio runtime, the L0 pipeline
//! (octoscript-ui-l0 check/realize → kit lower → eval → `to_dsl` → a
//! `Splash` per card), sessions, the composer, and the kernel agent
//! (`liboctos.so serve --stdio` from this APK's nativeLibraryDir when it is
//! bundled, else the WebSocket transport / the login screen).
//!
//! `register` runs the app's script_mods in the instance isolate and
//! registers the framework's `sys`/`agent` engine as a Splash isolate mod
//! (the cards' isolates are minted without it); `create` mints
//! `octos_app::AppShell` — a widget that owns the app's state, delivers
//! `Event::Startup` to it on first contact (that is where the agent and the
//! kernel transport come up, exactly as in the APK) and draws the app's root
//! body without its `Window{}`. The host's tile is the window.
//!
//! The kernel is the app's own affair: `octos_app`'s `stdio_spawn` finds
//! `liboctos.so` in this APK's nativeLibraryDir (bundled with
//! `MAKEPAD_ANDROID_EXTRA_LIBS`, see docs/android-appcard-build.md), gives it
//! `HOME=<files>/octos-home` and the memory budget, and runs `serve --stdio`
//! as a `kill_on_drop` child of the agent's runtime — so this module spawns
//! no kernel of its own (the Phase-A probe would have been a second child).
//!
//! What stays host-owned: the OS window and keyboard insets, and the
//! notifications / share / WebView overlay of the standalone APK's Java
//! activity (GPS already reaches `makepad_platform::gps` through the
//! buildtool activity).
pub use makepad_widgets;
pub use octos_app;

use makepad_app_module::{
    makepad_ai_services::wire::{Risk, ServiceCall, ServiceManifest, ToolDef, ToolResult},
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, OpenSchema, ServiceExecutor, ValidatedOpen,
};
use makepad_widgets::*;
use octos_app::AppShell;

pub struct AppCardModule;
pub static APPCARD_MODULE: AppCardModule = AppCardModule;

/// A host's live-activity sink (OctoSense's island): told about the kernel
/// turns this module submits. `begin` fires from the `ask` tool with the
/// call id as the activity id; `progress` and `finish` are here for the
/// day octos-app exposes its turn lifecycle to a host — today a turn's end
/// is read back through [`turn_in_flight`], and turns typed into the card's
/// own composer never pass through this crate at all (TODO: an
/// `AppShell` turn callback in octos-app would cover both).
pub trait ActivityReporter: Send + Sync {
    fn begin(&self, id: &str, title: &str);
    fn progress(&self, id: &str, done: usize, total: usize, detail: &str);
    fn finish(&self, id: &str);
}

static ACTIVITY_REPORTER: std::sync::OnceLock<Box<dyn ActivityReporter>> = std::sync::OnceLock::new();

/// Install the host's reporter, once. False when one is already there.
pub fn set_activity_reporter(reporter: Box<dyn ActivityReporter>) -> bool {
    ACTIVITY_REPORTER.set(reporter).is_ok()
}

/// The reporter the host installed, if any.
pub fn activity_reporter() -> Option<&'static dyn ActivityReporter> {
    ACTIVITY_REPORTER.get().map(|r| r.as_ref())
}

/// Archive the hosted app's card approval store once per host build, the
/// way a deployment of the standalone APK does it.
///
/// octos-app pins every admitted card to the runtime bundle it was admitted
/// under (`l0_approval_store::require` with `SPLASH_RUNTIME_BUNDLE`) and
/// fails closed on a stale receipt — a card then draws nothing. Archiving the
/// store is the host's EXPLICIT deployment action (octos-app's own
/// `MAKEPAD_REAPPROVE_CARDS` does exactly this rename at startup), so the
/// host does it itself when its build id changed: the store moves to
/// `l0-approvals-before-studio-<build_id>` beside itself, the same name the
/// app uses, and the id is remembered in `<config>/octosense-host-build` so
/// the next launch of the same build leaves the fresh receipts alone.
///
/// `config` is octos-app's config directory (`$OCTOS_APP_CONFIG_DIR`, else
/// `$HOME/.config/octos-app`) — [`octos_app_config_dir`] finds it. Returns
/// what was archived: `None` when this build already did it, or there was
/// nothing to archive and the marker is now written.
pub fn reapprove_cards_for_host_build(config: &std::path::Path, build_id: &str) -> Result<Option<std::path::PathBuf>, String> {
    if build_id.is_empty() || !build_id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("host build id {build_id:?} is not digits"));
    }
    let marker = config.join("octosense-host-build");
    if std::fs::read_to_string(&marker).map(|s| s.trim() == build_id).unwrap_or(false) {
        return Ok(None);
    }
    let store = config.join("l0-approvals");
    let backup = config.join(format!("l0-approvals-before-studio-{build_id}"));
    let archived = if store.is_dir() && !backup.exists() {
        std::fs::rename(&store, &backup).map_err(|e| format!("archive card receipts {}: {e}", store.display()))?;
        Some(backup)
    } else {
        None
    };
    std::fs::create_dir_all(config).map_err(|e| format!("create {}: {e}", config.display()))?;
    std::fs::write(&marker, build_id).map_err(|e| format!("write {}: {e}", marker.display()))?;
    Ok(archived)
}

/// Where octos-app keeps its config — its own rule, restated: the
/// `OCTOS_APP_CONFIG_DIR` override, else `<home>/.config/octos-app`, where
/// `home` is what the app itself makes `$HOME` at its startup: on a phone
/// the platform's data dir (`cx.get_data_dir()`, `<files>` on Android) —
/// the host asks BEFORE the app has started, so `$HOME` alone would be
/// unset there — else the process's `$HOME`.
pub fn octos_app_config_dir(data_dir: Option<String>) -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("OCTOS_APP_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return Some(std::path::PathBuf::from(dir));
    }
    let home = match data_dir.filter(|d| !d.is_empty() && cfg!(any(target_os = "android", target_env = "ohos"))) {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::path::PathBuf::from(std::env::var_os("HOME")?),
    };
    Some(home.join(".config").join("octos-app"))
}

/// The kernel turn in flight, as the app's chat state knows it: the
/// prompt text that started it (empty when the app has no record of it),
/// `None` when no turn is running. Set at submit — the `ask` tool and the
/// composer alike — and cleared when the turn completes or fails.
pub fn turn_in_flight() -> Option<String> {
    let data = octos_app::CHAT_DATA.read().ok()?;
    if !data.is_streaming {
        return None;
    }
    Some(
        data.messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, octos_app::ChatRole::User))
            .map(|m| m.text.clone())
            .unwrap_or_default(),
    )
}

impl AppModule for AppCardModule {
    fn id(&self) -> &'static str { "appcard" }
    fn label(&self) -> &'static str { "AppCard" }
    /// The app's own script_mods (widget prototypes, the code-editor and
    /// diagram kits, the root body) in the isolate the host prepared — the
    /// widgets' own `script_mod` has already run there.
    fn register(&self, vm: &mut ScriptVm) {
        octos_app::register_script_mods(vm);
        // The app's cards are Splash widgets, each in an isolate of its own,
        // and an isolate is minted with `widgets_mod` but WITHOUT the
        // framework's `sys`/`agent` engine (`register_agent_module`, which
        // `widgets::script_mod` installs into the main VM only) — and
        // octos-app's `register_script_mods` does not install it either.
        // Registered as a host isolate mod, so every card isolate minted from
        // now on carries it: without it a body's `sys.weather(...)` is "not
        // found in scope" and the card draws nothing, silently. Registering
        // on each `register` is harmless: a second install rebinds the same
        // names. The host test `appcard_isolates_carry_the_sys_engine_after_register`
        // guards this.
        makepad_widgets::widget_async::register_splash_isolate_mod(makepad_widgets::splash::register_agent_module);
    }
    fn open_schema(&self) -> OpenSchema { OpenSchema::new(1) }
    /// Cards fetch live values over HTTP; the store keeps sessions and
    /// cursors on disk; the agent talks to the kernel (a child, or a socket).
    fn capabilities(&self) -> &'static [&'static str] { &["storage", "net"] }
    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        let root = AppShell::create(vm);
        let shell = root.clone();
        InstanceParts {
            root: root.clone(),
            executor: Box::new(AppCardExecutor { root }),
            // The host runs this before it drops the root: stop the agent —
            // its tokio runtime and the kernel child (kill_on_drop) go with
            // it — so nothing outlives the isolate.
            shutdown: Box::new(move |_vm| {
                if let Some(mut inner) = shell.borrow_mut::<AppShell>() {
                    inner.shutdown();
                }
            }),
        }
    }
}

/// The instance's tools over the AI bus: `ask` is the composer.
struct AppCardExecutor {
    root: WidgetRef,
}

pub fn manifest() -> ServiceManifest {
    ServiceManifest::new(
        "appcard",
        "AppCard",
        "Octoscript-AppCard: ask for anything and get a live app card (weather, news, stocks, nav, ...).",
    )
    .with_tool(ToolDef::new(
        "ask",
        "Submit a request exactly as if typed into AppCard's composer: the routing brain picks the app and renders its card.",
        r#"{"type":"object","properties":{"text":{"type":"string","description":"The request, e.g. \"weather tokyo\""}},"required":["text"]}"#,
        Risk::Act,
    ))
}

impl ServiceExecutor for AppCardExecutor {
    fn manifest(&self) -> ServiceManifest {
        manifest()
    }
    fn execute(&mut self, cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        let result = match call.tool.as_str() {
            "ask" => {
                let text = serde_json::from_str::<serde_json::Value>(&call.args)
                    .ok()
                    .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(str::to_string))
                    .unwrap_or_default();
                if text.trim().is_empty() {
                    ToolResult::failed(&call.call_id, "`text` is required")
                } else {
                    let submitted = self
                        .root
                        .borrow_mut::<AppShell>()
                        .map(|mut shell| shell.ask(cx, &text))
                        .unwrap_or(false);
                    if submitted {
                        // The host's island shows the turn from here; its
                        // end is read back through `turn_in_flight`.
                        if let Some(reporter) = activity_reporter() {
                            reporter.begin(&call.call_id, &text);
                        }
                        ToolResult::ok(&call.call_id, format!("submitted: {text}"), "")
                    } else {
                        ToolResult::unavailable(&call.call_id, "AppCard is not running")
                    }
                }
            }
            other => ToolResult::unavailable(&call.call_id, format!("AppCard has no tool `{other}`")),
        };
        ExecOutcome::Done(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_describes_itself_and_opens_empty() {
        let m = &APPCARD_MODULE;
        assert_eq!(m.id(), "appcard");
        assert_eq!(m.label(), "AppCard");
        assert_eq!(m.capabilities(), &["storage", "net"]);
        assert!(m.open_schema().empty_open().is_ok());
        let manifest = manifest();
        assert!(manifest.tool("ask").is_some(), "the composer is a tool");
        assert_eq!(manifest.tools.len(), 1);
    }

    /// The whole app in a fresh isolate, the way the host seats it: no
    /// script errors, a real shell with an app in it, and a clean teardown.
    #[test]
    fn the_shell_builds_in_an_isolate_without_script_errors() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let vm_id = cx.alloc_splash_vm_with_network(false);
        let parts = cx.with_script_vm_id_trusted(vm_id, |vm| {
            APPCARD_MODULE.register(vm);
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "register left script errors: {errors:?}");
            let open = APPCARD_MODULE.open_schema().empty_open().unwrap();
            let (replies, _rx) = makepad_app_module::ReplySink::pair();
            let handles = InstanceHandles {
                scope: makepad_app_module::InstanceScope::new(1, 1),
                storage: cx_storage_for_test(vm),
                viewport: makepad_app_module::Viewport { size: dvec2(400.0, 700.0) },
                replies,
            };
            let parts = APPCARD_MODULE.create(vm, open, handles);
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "create left script errors: {errors:?}");
            parts
        });
        assert!(!parts.root.is_empty());
        assert!(parts.root.borrow::<AppShell>().map(|s| s.has_app()).unwrap_or(false));
        assert!(parts.executor.manifest().tool("ask").is_some());
        let shutdown = parts.shutdown;
        cx.with_script_vm_id_trusted(vm_id, |vm| shutdown(vm));
        drop(parts.root);
        drop(parts.executor);
        cx.free_splash_vm(vm_id);
    }

    fn cx_storage_for_test(vm: &mut ScriptVm) -> makepad_widgets::makepad_platform::storage::StorageHandle {
        vm.cx_mut().storage("appcard.test")
    }

    /// A new host build archives the store once, under the app's own backup
    /// name; the same build again leaves the fresh receipts alone; a bad id
    /// is refused before anything moves.
    #[test]
    fn a_new_host_build_archives_the_card_approvals_once() {
        let dir = std::env::temp_dir().join(format!("octosense-appcard-reapprove-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = dir.join("l0-approvals");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::write(store.join("receipt.json"), "{}").unwrap();
        assert!(reapprove_cards_for_host_build(&dir, "abc").is_err(), "an id must be digits");
        assert!(store.is_dir(), "a refused id moves nothing");
        let backup = reapprove_cards_for_host_build(&dir, "1700000000").unwrap().expect("the first launch of a build archives");
        assert_eq!(backup, dir.join("l0-approvals-before-studio-1700000000"));
        assert!(backup.join("receipt.json").is_file() && !store.exists(), "the receipts moved aside intact");
        // The app admits cards again into a fresh store...
        std::fs::create_dir_all(&store).unwrap();
        std::fs::write(store.join("fresh.json"), "{}").unwrap();
        // ...which the same build never touches, however often it launches.
        assert_eq!(reapprove_cards_for_host_build(&dir, "1700000000").unwrap(), None);
        assert!(store.join("fresh.json").is_file());
        // The next build archives again, under its own name.
        assert!(reapprove_cards_for_host_build(&dir, "1700000001").unwrap().is_some());
        assert!(!store.exists() && dir.join("l0-approvals-before-studio-1700000001").join("fresh.json").is_file());
        // No store at all: nothing to archive, but the build is remembered.
        assert_eq!(reapprove_cards_for_host_build(&dir, "1700000002").unwrap(), None);
        assert_eq!(std::fs::read_to_string(dir.join("octosense-host-build")).unwrap(), "1700000002");
        assert_eq!(reapprove_cards_for_host_build(&dir, "1700000002").unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
