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
                windows: Default::default(),
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
}
